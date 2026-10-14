/* 鸿升飞书 · 聊天组件（桌面独立窗口 / 移动端覆盖层共用）
 * 用法：HSFChat.createChat(mountEl, uid, { embedded, onClose })
 */
(function () {
  "use strict";

  const ICON = {
    close: '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round"><path d="M18 6 6 18M6 6l12 12"/></svg>',
    min: '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round"><path d="M5 12h14"/></svg>',
    lock: '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="11" width="18" height="11" rx="2"/><path d="M7 11V7a5 5 0 0 1 10 0v4"/></svg>',
    unlock: '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="11" width="18" height="11" rx="2"/><path d="M7 11V7a5 5 0 0 1 9.9-1"/></svg>',
    clip: '<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21.44 11.05l-9.19 9.19a6 6 0 0 1-8.49-8.49l8.57-8.57A4 4 0 1 1 18 8.84l-8.59 8.57a2 2 0 0 1-2.83-2.83l8.49-8.48"/></svg>',
    send: '<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m22 2-7 20-4-9-9-4 20-7z"/></svg>',
    back: '<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m15 18-6-6 6-6"/></svg>',
    key: '<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="7.5" cy="15.5" r="4.5"/><path d="m17 7 3 3-8 8H9v-3z"/></svg>',
    folder: '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/></svg>',
    ext: '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6"/><path d="M15 3h6v6"/><path d="M10 14 21 3"/></svg>',
  };

  const STATUS_TEXT = {
    sending: "发送中",
    sent: "已发送",
    delivered: "已送达",
    receiving: "接收中",
    received: "已接收",
    failed: "发送失败",
    rejected: "已拒绝",
  };

  function el(tag, cls, html) {
    const e = document.createElement(tag);
    if (cls) e.className = cls;
    if (html !== undefined) e.innerHTML = html;
    return e;
  }

  function esc(s) {
    return HSF.escapeHtml(s);
  }

  /* ---------- 单个聊画面 ---------- */
  function createChat(mount, uid, opts) {
    opts = opts || {};
    const embedded = !!opts.embedded;
    let peer = null;
    let settings = null;
    let selfNick = "我";
    let messages = [];
    let encPass = null; // 口令（内存保留，切换锁图标状态）
    const unsubs = [];

    mount.classList.add("chat");
    mount.innerHTML = `
      <header class="chat-head">
        ${
          embedded
            ? `<button class="c-btns" id="cBack" style="border:none;background:none;color:var(--text);width:30px;height:34px;cursor:pointer;display:flex;align-items:center">${ICON.back}</button>`
            : ""
        }
        <div class="avatar sm" id="cAvatar">?</div>
        <div class="peer-meta">
          <div class="peer-name" id="cName">—</div>
          <div class="peer-sub" id="cSub">—</div>
        </div>
        <div class="head-actions">
          ${
            embedded
              ? ""
              : `<button id="cMin" title="最小化">${ICON.min}</button>`
          }
          <button id="cLock" title="口令加密">${ICON.unlock}</button>
          ${
            embedded
              ? `<button id="cCloseOv" title="关闭">${ICON.close}</button>`
              : `<button id="cClose" title="关闭">${ICON.close}</button>`
          }
        </div>
      </header>
      <div class="messages" id="cMessages"></div>
      <div class="composer">
        <div class="c-btns">
          <button id="cAttach" title="发送文件 / 文件夹">${ICON.clip}</button>
          <button id="cLockBtn" title="口令加密">${ICON.unlock}</button>
        </div>
        <textarea id="cInput" rows="1" placeholder="输入消息，Enter 发送，Shift+Enter 换行"></textarea>
        <button class="send-btn" id="cSend">${ICON.send}<span>发送</span></button>
      </div>
      <div class="drop-mask" id="cDrop"><div class="dm-inner">松开以发送文件 / 文件夹</div></div>
      <input type="file" id="cFile" multiple hidden />
    `;

    const $ = (id) => mount.querySelector("#" + id);
    const msgBox = $("cMessages");
    const input = $("cInput");

    /* ---- 头部 ---- */
    async function refreshHead() {
      try {
        const peers = await HSF.invoke("list_peers");
        peer = peers.find((p) => p.uid === uid) || null;
      } catch (e) {
        peer = null;
      }
      const av = $("cAvatar");
      const name = $("cName");
      const sub = $("cSub");
      if (peer) {
        const alias = settings && settings.aliases && settings.aliases[uid];
        const dn = (alias && alias.trim()) || peer.nick || peer.host || "未知";
        av.textContent = dn.trim().charAt(0);
        av.style.background = HSF.avatarBg(peer.uid);
        name.textContent = dn;
        sub.textContent = `${peer.host || ""} · ${peer.ip} · ${HSF.osLabel(peer.os)}`;
        const dotCls = peer.status === "away" ? "away" : peer.status === "invisible" ? "invisible" : "";
        if (!av.querySelector(".dot")) av.appendChild(el("span", "dot " + dotCls));
        else av.querySelector(".dot").className = "dot " + dotCls;
      } else {
        av.textContent = "?";
        name.textContent = "离线联系人";
        sub.textContent = "暂不在线，消息将在其上线后送达";
      }
    }

    /* ---- 消息渲染 ---- */
    function msgNode(m) {
      const row = el("div", "msg-row " + m.direction);
      const av = el("div", "avatar sm", esc(senderName(m).charAt(0)));
      av.style.background = HSF.avatarBg(uid);
      row.appendChild(av);

      const bubble = el("div", "msg-bubble");
      if (m.kind === "file") {
        bubble.appendChild(fileCard(m));
      } else {
        const t = el("div", "bubble-text");
        // 仅接收到的密文需要本地解密；自己发出的密文消息本地保存的即为明文
        if (m.enc && m.direction === "in") {
          const c = el("div", "cipher", esc(m.text));
          t.appendChild(c);
          const dec = el("button", "", `${ICON.key} 解密`);
          dec.style.cssText =
            "border:none;background:rgba(0,0,0,.08);border-radius:6px;padding:2px 8px;font-size:11px;margin-top:6px;cursor:pointer;color:inherit";
          dec.onclick = async () => {
            if (!encPass) {
              encPass = await HSF.promptModal({
                title: "输入通信口令",
                placeholder: "与对方约定的口令",
                password: true,
              });
              if (!encPass) return;
            }
            const plain = await HSF.invoke("decrypt_text", { cipherB64: m.text, pass: encPass });
            if (plain) {
              c.textContent = plain;
              dec.remove();
            } else {
              HSF.toast("口令错误，无法解密");
            }
          };
          t.appendChild(dec);
        } else {
          t.textContent = m.text;
        }
        bubble.appendChild(t);
      }
      const meta = el("div", "msg-meta");
      meta.innerHTML = `<span>${HSF.fmtTime(m.ts)}</span>`;
      if (m.enc && m.kind === "text") {
        meta.appendChild(el("span", "enc-flag", `${ICON.lock} 加密`));
      }
      if (m.direction === "out") {
        const st = el("span", "st " + m.status, esc(STATUS_TEXT[m.status] || m.status || ""));
        meta.appendChild(st);
      }
      bubble.appendChild(meta);
      row.appendChild(bubble);
      return row;
    }

    function senderName(m) {
      if (m.direction === "in") return (peer && peer.nick) || "?";
      return selfNick || "我";
    }

    function fileCard(m) {
      const card = el("div", "file-card");
      card.dataset.transfer = m.transfer_id;
      const icon = el("div", "fi", m.is_dir ? "📁" : HSF.fileIcon(m.file_name || "")[0]);
      card.appendChild(icon);
      const mid = el("div", "fm");
      mid.innerHTML = `
        <div class="fname">${esc(m.file_name || "未命名文件")}${m.is_dir ? "（文件夹）" : ""}</div>
        <div class="fsize">${HSF.fmtSize(m.file_size)}</div>
        <div class="progress"><i style="width:${m.progress || 0}%"></i></div>
        <div class="progress-line"><span class="pstate">${esc(STATUS_TEXT[m.status] || m.status || "")}</span><span class="pspeed"></span></div>
      `;
      card.appendChild(mid);
      const ops = el("div", "fops");
      card.appendChild(ops);
      updateFileCard(card, m);
      return card;
    }

    function updateFileCard(card, m) {
      const state = card.querySelector(".pstate");
      const speed = card.querySelector(".pspeed");
      const bar = card.querySelector(".progress > i");
      const ops = card.querySelector(".fops");
      state.textContent = STATUS_TEXT[m.status] || m.status || "";
      bar.style.width = (m.progress || 0) + "%";
      if (m.status === "sending" || m.status === "receiving") {
        speed.textContent = m.speed_bps
          ? `${HSF.fmtSpeed(m.speed_bps)}${m.eta_secs ? " · 剩余 " + HSF.fmtEta(m.eta_secs) : ""}`
          : "";
      } else {
        speed.textContent = "";
      }
      ops.innerHTML = "";
      if (m.status === "received") {
        const open = el("button", "acc", `${ICON.ext} 打开`);
        open.title = "用默认程序打开";
        open.onclick = () => HSF.invoke("open_path", { path: m.saved_path || "", reveal: false }).catch((e) => HSF.toast(e));
        const folder = el("button", "", `${ICON.folder} 文件夹`);
        folder.title = "在文件管理器中显示";
        folder.onclick = () => HSF.invoke("open_path", { path: m.saved_path || "", reveal: true }).catch((e) => HSF.toast(e));
        ops.appendChild(open);
        ops.appendChild(folder);
      }
    }

    function upsert(m) {
      const idx = messages.findIndex((x) => x.id === m.id);
      if (idx >= 0) messages[idx] = m;
      else messages.push(m);
      const exist = msgBox.querySelector(`[data-mid="${m.id}"]`);
      if (exist) {
        const fresh = msgNode(m);
        fresh.dataset.mid = m.id;
        if (m.kind === "file") {
          const card = fresh.querySelector(".file-card");
          updateFileCard(card, m);
          const old = exist.querySelector(".file-card");
          if (old) old.replaceWith(card);
          const st = exist.querySelector(".msg-meta .st");
          if (st) {
            st.className = "st " + m.status;
            st.textContent = STATUS_TEXT[m.status] || m.status;
          }
        } else {
          const st = exist.querySelector(".msg-meta .st");
          if (st) {
            st.className = "st " + m.status;
            st.textContent = STATUS_TEXT[m.status] || m.status;
          }
        }
      } else {
        const node = msgNode(m);
        node.dataset.mid = m.id;
        msgBox.appendChild(node);
      }
      msgBox.scrollTop = msgBox.scrollHeight;
    }

    /* ---- 加载历史 ---- */
    async function loadHistory() {
      try {
        messages = await HSF.invoke("list_messages", { uid });
      } catch (e) {
        messages = [];
      }
      msgBox.innerHTML = "";
      messages.forEach((m) => {
        const node = msgNode(m);
        node.dataset.mid = m.id;
        msgBox.appendChild(node);
      });
      msgBox.scrollTop = msgBox.scrollHeight;
    }

    /* ---- 发送 ---- */
    async function doSend() {
      const text = input.value.replace(/\s+$/, "");
      if (!text.trim()) return;
      input.value = "";
      autoGrow();
      try {
        await HSF.invoke("send_text", { peerUid: uid, text, pass: encPass });
      } catch (e) {
        HSF.toast("发送失败：" + e);
      }
    }

    function autoGrow() {
      input.style.height = "auto";
      input.style.height = Math.min(input.scrollHeight, 120) + "px";
    }

    /* ---- 事件订阅 ---- */
    function onMessage(ev) {
      const m = ev.payload;
      if (!m || m.peer_uid !== uid) return;
      if (m.direction === "in") HSF.playSound("msg");
      upsert(m);
    }
    const unMsg = HSF.listen("message", onMessage);
    unsubs.push(() => Promise.resolve(unMsg).then((f) => f && f()));

    /* ---- 交互绑定 ---- */
    $("cSend").onclick = doSend;
    input.addEventListener("keydown", (e) => {
      if (e.key === "Enter" && !e.shiftKey) {
        e.preventDefault();
        doSend();
      }
    });
    input.addEventListener("input", autoGrow);

    // 锁：开启后发送走口令加密
    function toggleLock() {
      if (encPass) {
        encPass = null;
        $("cLock").innerHTML = ICON.unlock;
        $("cLockBtn").innerHTML = ICON.unlock;
        $("cLock").classList.remove("on");
        $("cLockBtn").classList.remove("on");
        HSF.toast("已关闭口令加密");
      } else {
        HSF.promptModal({
          title: "开启口令加密",
          placeholder: "输入与对方约定的口令",
          password: true,
        }).then((p) => {
          if (!p) return;
          encPass = p;
          $("cLock").innerHTML = ICON.lock;
          $("cLockBtn").innerHTML = ICON.lock;
          $("cLock").classList.add("on");
          $("cLockBtn").classList.add("on");
          HSF.toast("已开启口令加密，双方需使用相同口令");
        });
      }
    }
    $("cLock").onclick = toggleLock;
    $("cLockBtn").onclick = toggleLock;

    if (!embedded) {
      $("cMin").onclick = () => HSF.invoke("win_minimize").catch(() => {});
      $("cClose").onclick = () => HSF.invoke("win_close").catch(() => {});
      HSF.invoke("register_chat_window", { uid }).catch(() => {});
    } else {
      $("cBack").onclick = close;
      $("cCloseOv").onclick = close;
    }

    function close() {
      unsubs.forEach((f) => f());
      if (!embedded) {
        HSF.invoke("unregister_chat_window", { uid }).catch(() => {});
        HSF.invoke("win_close").catch(() => {});
      }
      mount.classList.remove("chat");
      mount.innerHTML = "";
      if (opts.onClose) opts.onClose();
    }

    /* ---- 文件发送 ---- */
    const fileInput = $("cFile");
    $("cAttach").onclick = async () => {
      // 桌面端用系统对话框拿真实路径；移动端用文件选择器走字节上传
      if (!HSF.IS_MOBILE) {
        try {
          const { open } = HSF.T.dialog;
          const selected = await open({ multiple: true });
          if (!selected) return;
          const paths = Array.isArray(selected) ? selected : [selected];
          for (const p of paths) {
            HSF.invoke("send_file", { peerUid: uid, path: p }).catch((e) =>
              HSF.toast("发送失败：" + e)
            );
          }
        } catch (e) {
          HSF.toast("无法打开文件选择器");
        }
        return;
      }
      fileInput.click();
    };
    fileInput.onchange = async () => {
      const files = Array.from(fileInput.files || []);
      fileInput.value = "";
      for (const f of files) await sendPathOrFile(f);
    };

    async function sendPathOrFile(f) {
      // 桌面端拖入的 File 有 path（Tauri webview）；否则走字节上传通道
      const p = f.path || f.webkitRelativePath || null;
      if (p && !HSF.IS_MOBILE) {
        try {
          await HSF.invoke("send_file", { peerUid: uid, path: p });
        } catch (e) {
          HSF.toast("发送失败：" + e);
        }
        return;
      }
      await uploadBytes(f);
    }

    async function uploadBytes(file) {
      try {
        const transferId = await HSF.invoke("begin_file_bytes", { name: file.name });
        const CHUNK = 512 * 1024;
        for (let off = 0; off < file.size; off += CHUNK) {
          const blob = file.slice(off, off + CHUNK);
          const b64 = await blobToB64(blob);
          await HSF.invoke("push_file_chunk", { transferId, data: b64 });
        }
        await HSF.invoke("finish_file_bytes", { transferId, peerUid: uid });
      } catch (e) {
        HSF.toast("发送失败：" + e);
      }
    }

    function blobToB64(blob) {
      return new Promise((resolve, reject) => {
        const r = new FileReader();
        r.onload = () => {
          const s = r.result;
          resolve(s.slice(s.indexOf(",") + 1));
        };
        r.onerror = reject;
        r.readAsDataURL(blob);
      });
    }

    /* ---- 拖拽发送（桌面） ---- */
    if (!embedded && !HSF.IS_MOBILE) {
      let dragDepth = 0;
      mount.addEventListener("dragenter", (e) => {
        e.preventDefault();
        dragDepth++;
        $("cDrop").classList.add("show");
      });
      mount.addEventListener("dragleave", (e) => {
        e.preventDefault();
        dragDepth--;
        if (dragDepth <= 0) $("cDrop").classList.remove("show");
      });
      mount.addEventListener("dragover", (e) => e.preventDefault());
      mount.addEventListener("drop", async (e) => {
        e.preventDefault();
        dragDepth = 0;
        $("cDrop").classList.remove("show");
        const items = Array.from(e.dataTransfer.items || []);
        const entries = items
          .filter((it) => it.kind === "file")
          .map((it) => (it.webkitGetAsEntry ? it.webkitGetAsEntry() : null))
          .filter(Boolean);
        if (entries.length) {
          for (const en of entries) {
            const p = await entryPath(en);
            if (p) await HSF.invoke("send_file", { peerUid: uid, path: p }).catch((err) => HSF.toast("发送失败：" + err));
          }
          return;
        }
        for (const f of Array.from(e.dataTransfer.files || [])) await sendPathOrFile(f);
      });
    }

    function entryPath(entry) {
      return new Promise((resolve) => {
        // Tauri v2 注入的 File.path 可直接用
        try {
          const anyEntry = entry;
          if (anyEntry.fullPath && anyEntry.filesystem) {
            resolve(anyEntry.fullPath);
            return;
          }
        } catch (e) {}
        entry.file(
          (f) => resolve(f.path || f.webkitRelativePath || null),
          () => resolve(null)
        );
      });
    }

    /* ---- 启动 ---- */
    HSF.invoke("get_self")
      .then((s) => {
        selfNick = (s && s.nick) || "我";
      })
      .catch(() => {});
    HSF.invoke("get_settings")
      .then((s) => {
        settings = s;
        refreshHead();
      })
      .catch(() => {});
    refreshHead();
    loadHistory();

    return {
      uid,
      close,
      refresh: refreshHead,
    };
  }

  window.HSFChat = { createChat };
})();
