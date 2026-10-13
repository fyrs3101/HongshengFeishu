// 鸿升飞书 · 前端逻辑（原生 JS，依赖 Tauri 全局 API）
// withGlobalTauri 已开启，window.__TAURI__ 提供 core / event / dialog / webview
const T = window.__TAURI__;
const { invoke } = T.core;
const { listen } = T.event;
const { dialog, webview } = T;

let peers = []; // [{uid,nick,host,ip,port,os}]
let activePeer = null; // uid
let messages = {}; // uid -> [msg]

const $ = (id) => document.getElementById(id);
const escapeHtml = (s) =>
  String(s).replace(/[&<>"']/g, (c) => ({
    "&": "&amp;",
    "<": "&lt;",
    ">": "&gt;",
    '"': "&quot;",
    "'": "&#39;",
  }[c]));

async function init() {
  try {
    await invoke("start");
  } catch (e) {
    console.error("start failed", e);
  }
  try {
    const self = await invoke("get_self");
    if (self && self.nick) $("selfNick").textContent = self.nick;
  } catch (e) {}
  try {
    peers = (await invoke("list_peers")) || [];
    renderPeers();
  } catch (e) {}

  await listen("peers", (e) => {
    peers = e.payload || [];
    renderPeers();
  });
  await listen("message", (e) => onMessage(e.payload));

  bindUI();
}

function renderPeers() {
  const kw = ($("searchInput").value || "").trim().toLowerCase();
  const list = $("peerList");
  const shown = peers.filter(
    (p) =>
      !kw ||
      (p.nick || "").toLowerCase().includes(kw) ||
      (p.host || "").toLowerCase().includes(kw) ||
      (p.ip || "").includes(kw)
  );
  $("onlineCount").textContent = String(peers.length);
  if (shown.length === 0) {
    list.innerHTML = `<li class="empty">${
      peers.length ? "无匹配联系人" : "正在发现网段内的同事…"
    }</li>`;
    return;
  }
  list.innerHTML = shown
    .map((p) => {
      const init = (p.nick || "?").trim().charAt(0) || "?";
      const active = p.uid === activePeer ? " active" : "";
      return `<li class="peer${active}" data-uid="${p.uid}">
        <div class="avatar">${escapeHtml(init)}<span class="dot"></span></div>
        <div class="peer-meta">
          <div class="peer-name">${escapeHtml(p.nick || "未知")}</div>
          <div class="peer-host">${escapeHtml(p.host || "")} · ${escapeHtml(p.ip || "")}</div>
        </div>
      </li>`;
    })
    .join("");
  list.querySelectorAll(".peer").forEach((el) => {
    el.onclick = () => openChat(el.dataset.uid);
  });
}

function openChat(uid) {
  const p = peers.find((x) => x.uid === uid);
  if (!p) return;
  activePeer = uid;
  $("chatEmpty").hidden = true;
  $("chatActive").hidden = false;
  $("headName").textContent = p.nick || "未知";
  $("headIp").textContent = `${p.host || ""} · ${p.ip || ""}`;
  const init = (p.nick || "?").charAt(0) || "?";
  $("headAvatar").textContent = init;
  renderPeers();
  renderMessages();
  $("input").focus();
}

function onMessage(msg) {
  if (!msg) return;
  const arr = messages[msg.peer_uid] || (messages[msg.peer_uid] = []);
  const idx = arr.findIndex((m) => m.id === msg.id);
  if (idx >= 0) arr[idx] = msg;
  else arr.push(msg);
  if (msg.peer_uid === activePeer) renderMessages();
  // 若收到新消息但当前未打开该会话，保持侧边栏红点（此处简化为刷新列表）
  if (msg.direction === "in" && msg.peer_uid !== activePeer) renderPeers();
}

function renderMessages() {
  const box = $("messages");
  const arr = messages[activePeer] || [];
  box.innerHTML = arr
    .map((m) => {
      const cls = `msg ${m.direction}`;
      if (m.kind === "file") {
        const pct = m.progress || 0;
        const bar =
          m.status === "sending" || m.status === "receiving"
            ? `<div class="progress"><i style="width:${pct}%"></i></div>`
            : "";
        return `<div class="${cls} file">
          <div>📎</div>
          <div>
            <div class="file-name">${escapeHtml(m.file_name || "文件")}</div>
            <div class="file-size">${fmtSize(m.file_size)} · ${statusText(m.status)}</div>
            ${bar}
          </div>
        </div>`;
      }
      return `<div class="${cls}">${escapeHtml(m.text || "")}</div>`;
    })
    .join("");
  box.scrollTop = box.scrollHeight;
}

function fmtSize(b) {
  if (!b) return "0 B";
  const u = ["B", "KB", "MB", "GB"];
  let i = 0;
  let n = b;
  while (n >= 1024 && i < u.length - 1) {
    n /= 1024;
    i++;
  }
  return `${i === 0 ? n : n.toFixed(1)} ${u[i]}`;
}

function statusText(s) {
  return (
    {
      sending: "发送中",
      sent: "已发送",
      receiving: "接收中",
      received: "已接收",
      failed: "失败",
    }[s] || s
  );
}

async function doSend() {
  const text = $("input").value.trim();
  if (!text || !activePeer) return;
  $("input").value = "";
  autoGrow();
  try {
    await invoke("send_text", { peer_uid: activePeer, text });
  } catch (e) {
    alert("发送失败：" + e);
  }
}

async function pickAndSend(dir) {
  if (!activePeer) {
    alert("请先选择联系人");
    return;
  }
  try {
    const path = await dialog.open(dir ? { directory: true } : { multiple: false });
    if (!path) return;
    const file = Array.isArray(path) ? path[0] : path;
    await invoke("send_file", { peer_uid: activePeer, path: file });
  } catch (e) {
    alert("发送失败：" + e);
  }
}

function autoGrow() {
  const ta = $("input");
  ta.style.height = "auto";
  ta.style.height = Math.min(ta.scrollHeight, 120) + "px";
}

function bindUI() {
  $("searchInput").oninput = renderPeers;
  $("sendBtn").onclick = doSend;
  $("input").oninput = autoGrow;
  $("input").onkeydown = (e) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      doSend();
    }
  };
  $("attachBtn").onclick = () => pickAndSend(false);
  $("settingsBtn").onclick = async () => {
    const nick = prompt("设置你的昵称：", $("selfNick").textContent);
    if (nick && nick.trim()) {
      try {
        await invoke("set_nick", { nick: nick.trim() });
        $("selfNick").textContent = nick.trim();
      } catch (e) {
        alert("设置失败：" + e);
      }
    }
  };

  // 拖拽发送（仅当已选中联系人）
  const overlay = $("dropOverlay");
  window.addEventListener("dragover", (e) => {
    if (activePeer) {
      e.preventDefault();
      overlay.classList.add("show");
    }
  });
  window.addEventListener("dragleave", (e) => {
    if (e.target === overlay) overlay.classList.remove("show");
  });
  window.addEventListener("drop", (e) => {
    e.preventDefault();
    overlay.classList.remove("show");
  });

  // Tauri webview 拖放事件（可拿到真实文件路径）
  try {
    webview.getCurrentWebview().onDragDropEvent((e) => {
      if (e.payload.type === "drop" && activePeer) {
        const file = e.payload.paths && e.payload.paths[0];
        if (file) invoke("send_file", { peer_uid: activePeer, path: file });
      }
    });
  } catch (e) {
    console.warn("drag-drop event unavailable", e);
  }
}

init();
