// 鸿升飞书 · 前端共享工具（依赖 Tauri 全局 API withGlobalTauri）
window.HSF = (() => {
  const T = window.__TAURI__;
  const invoke = (cmd, args) => T.core.invoke(cmd, args);
  const listen = (ev, cb) => T.event.listen(ev, cb);
  // 统一退订：返回 Promise<UnlistenFn>
  const unlisten = (ev, cb) => T.event.unlisten(ev, cb).catch(() => {});
  const IS_MOBILE =
    /Android|iPhone|iPad|iPod/i.test(navigator.userAgent) || window.innerWidth < 760;

  const $ = (sel, root) => (root || document).querySelector(sel);
  const $$ = (sel, root) => Array.from((root || document).querySelectorAll(sel));

  const escapeHtml = (s) =>
    String(s ?? "").replace(/[&<>"']/g, (c) => ({
      "&": "&amp;",
      "<": "&lt;",
      ">": "&gt;",
      '"': "&quot;",
      "'": "&#39;",
    }[c]));

  function fmtSize(b) {
    if (!b || b < 0) return "0 B";
    const u = ["B", "KB", "MB", "GB", "TB"];
    let i = 0;
    let n = b;
    while (n >= 1024 && i < u.length - 1) {
      n /= 1024;
      i++;
    }
    return `${i === 0 ? n : n.toFixed(1)} ${u[i]}`;
  }

  function fmtSpeed(bps) {
    if (!bps || bps <= 0) return "";
    return fmtSize(bps) + "/s";
  }

  function fmtEta(secs) {
    if (!secs || secs <= 0 || secs > 3600 * 24) return "";
    if (secs < 60) return `剩余 ${secs} 秒`;
    return `剩余 ${Math.ceil(secs / 60)} 分钟`;
  }

  function fmtTime(ts) {
    if (!ts) return "";
    const d = new Date(ts);
    const p = (n) => String(n).padStart(2, "0");
    return `${p(d.getHours())}:${p(d.getMinutes())}`;
  }

  function fmtDay(ts) {
    const d = new Date(ts);
    const today = new Date();
    const p = (n) => String(n).padStart(2, "0");
    if (d.toDateString() === today.toDateString()) return `今天 ${p(d.getHours())}:${p(d.getMinutes())}`;
    return `${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`;
  }

  // ---- 提示音 ----
  let soundOn = true;
  function setSound(on) {
    soundOn = on;
  }
  function playSound(name) {
    if (!soundOn) return;
    try {
      const a = new Audio(`assets/${name}.wav`);
      a.volume = 0.55;
      a.play().catch(() => {});
    } catch (e) {}
  }

  // ---- 轻提示 ----
  function toast(msg, type = "info", ms = 2600) {
    let wrap = $("#toastWrap");
    if (!wrap) {
      wrap = document.createElement("div");
      wrap.id = "toastWrap";
      wrap.className = "toast-wrap";
      document.body.appendChild(wrap);
    }
    const el = document.createElement("div");
    el.className = `toast${type === "error" || type === "err" ? " err" : ""}`;
    el.textContent = msg;
    wrap.appendChild(el);
    setTimeout(() => {
      el.style.opacity = "0";
      el.style.transition = "opacity .3s";
      setTimeout(() => el.remove(), 320);
    }, ms);
  }

  // ---- 模态框 ----
  function showModal({ title, html = "", buttons = [{ text: "知道了", primary: true }] }) {
    return new Promise((resolve) => {
      const mask = document.createElement("div");
      mask.className = "modal-mask";
      mask.innerHTML = `
        <div class="modal">
          <div class="m-head">${escapeHtml(title)}</div>
          ${html ? `<div class="m-body">${html}</div>` : ""}
          <div class="m-foot">
            ${buttons
              .map(
                (b, i) =>
                  `<button class="btn ${b.primary ? "primary" : ""} ${b.danger ? "danger" : ""}" data-i="${i}">${escapeHtml(b.text)}</button>`
              )
              .join("")}
          </div>
        </div>`;
      document.body.appendChild(mask);
      const close = (v) => {
        mask.remove();
        resolve(v);
      };
      mask.addEventListener("mousedown", (e) => {
        if (e.target === mask) close(null);
      });
      $$(".m-foot .btn", mask).forEach((btn) => {
        btn.onclick = () => {
          const b = buttons[+btn.dataset.i];
          close(b.value !== undefined ? b.value : +btn.dataset.i);
        };
      });
    });
  }

  // 输入弹窗：promptModal({ title, label, placeholder, value, password })
  function promptModal({
    title,
    label = "",
    placeholder = "",
    value = "",
    password = false,
  } = {}) {
    return showModal({
      title,
      html: `<label class="field"><span>${escapeHtml(label || title)}</span>
        <input type="${password ? "password" : "text"}" id="__pmInput" placeholder="${escapeHtml(
        placeholder
      )}" value="${escapeHtml(value)}"></label>`,
      buttons: [
        { text: "取消" },
        { text: "确定", primary: true, value: "__ok" },
      ],
    }).then((v) => {
      if (v !== "__ok") return null;
      const input = document.getElementById("__pmInput");
      return input ? input.value : "";
    });
  }

  // 文件接收确认弹窗（飞鸽风格）
  function fileOfferModal({ nick, name, size, isDir }) {
    return showModal({
      title: "文件接收确认",
      html: `<p style="margin:0 0 4px"><b>${escapeHtml(nick || "对方")}</b> 想要向你发送${
        isDir ? "文件夹" : "文件"
      }：</p>
      <div class="m-file">
        <span class="fi">${isDir ? "📁" : fileIcon(name || "")[0]}</span>
        <span><div class="fname">${escapeHtml(name || "未命名")}</div><div class="fsize">${fmtSize(
        size || 0
      )}</div></span>
      </div>
      <p style="margin:10px 0 0;color:var(--muted);font-size:12px">接收后存入收件箱（可在设置中修改目录）。</p>`,
      buttons: [
        { text: "拒绝", value: "reject" },
        { text: "接收", primary: true, value: "accept" },
      ],
    });
  }

  // ---- 右键菜单 ----
  function showContextMenu(x, y, items) {
    closeContextMenu();
    const menu = document.createElement("div");
    menu.className = "ctx-menu";
    menu.id = "__ctxMenu";
    menu.innerHTML = items
      .map(
        (it, i) =>
          `<button class="${it.danger ? "danger" : ""}" data-i="${i}">${escapeHtml(it.label)}</button>`
      )
      .join("");
    document.body.appendChild(menu);
    const rect = menu.getBoundingClientRect();
    const mx = Math.min(x, window.innerWidth - rect.width - 8);
    const my = Math.min(y, window.innerHeight - rect.height - 8);
    menu.style.left = mx + "px";
    menu.style.top = my + "px";
    $$("button", menu).forEach((el) => {
      el.onclick = () => {
        closeContextMenu();
        items[+el.dataset.i].onClick?.();
      };
    });
    setTimeout(() => document.addEventListener("mousedown", outside), 0);
    function outside(e) {
      if (!menu.contains(e.target)) closeContextMenu();
    }
  }
  function closeContextMenu() {
    const m = document.getElementById("__ctxMenu");
    if (m) m.remove();
  }

  // ---- 杂项 ----
  function avatarColor(name) {
    const colors = [
      ["#5b8def", "#3b6fd4"],
      ["#f2994a", "#e07c2c"],
      ["#27ae60", "#1e8b4d"],
      ["#9b59b6", "#7d4591"],
      ["#e05c6e", "#c23b50"],
      ["#12b5c9", "#0b8a9c"],
      ["#f2c94c", "#d9a912"],
      ["#6b7a99", "#4c5978"],
    ];
    let h = 0;
    for (const ch of String(name || "?")) h = (h * 31 + ch.codePointAt(0)) >>> 0;
    return colors[h % colors.length];
  }

  // 头像背景（渐变字符串）
  function avatarBg(name) {
    const c = avatarColor(name);
    return `linear-gradient(135deg, ${c[0]}, ${c[1]})`;
  }

  // 系统标识
  function osLabel(os) {
    const map = {
      macos: "macOS",
      windows: "Windows",
      linux: "Linux",
      android: "Android",
      ios: "iOS",
    };
    return map[os] || os || "未知";
  }

  function fileIcon(name) {
    const ext = (name.split(".").pop() || "").toLowerCase();
    const map = {
      zip: ["📦", "#b8860b"], rar: ["📦", "#b8860b"], "7z": ["📦", "#b8860b"],
      png: ["🖼️", "#8e44ad"], jpg: ["🖼️", "#8e44ad"], jpeg: ["🖼️", "#8e44ad"],
      gif: ["🖼️", "#8e44ad"], webp: ["🖼️", "#8e44ad"], bmp: ["🖼️", "#8e44ad"],
      mp4: ["🎬", "#c0392b"], mov: ["🎬", "#c0392b"], avi: ["🎬", "#c0392b"], mkv: ["🎬", "#c0392b"],
      mp3: ["🎵", "#16a085"], wav: ["🎵", "#16a085"], flac: ["🎵", "#16a085"],
      pdf: ["📕", "#c0392b"], doc: ["📘", "#2980b9"], docx: ["📘", "#2980b9"],
      xls: ["📗", "#27ae60"], xlsx: ["📗", "#27ae60"], csv: ["📗", "#27ae60"],
      ppt: ["📙", "#d35400"], pptx: ["📙", "#d35400"],
      txt: ["📄", "#7f8c8d"], md: ["📄", "#7f8c8d"], log: ["📄", "#7f8c8d"],
      exe: ["⚙️", "#34495e"], dmg: ["💿", "#34495e"], apk: ["🤖", "#27ae60"],
      js: ["📜", "#f1c40f"], ts: ["📜", "#f1c40f"], py: ["📜", "#f1c40f"],
      rs: ["📜", "#de8c17"], json: ["📋", "#7f8c8d"], html: ["🌐", "#e67e22"],
    };
    return map[ext] || ["📄", "#7f8c8d"];
  }

  const ICONS = {
    refresh:
      '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 12a9 9 0 1 1-2.64-6.36"/><polyline points="21 3 21 9 15 9"/></svg>',
    chat:
      '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 11.5a8.38 8.38 0 0 1-.9 3.8 8.5 8.5 0 0 1-7.6 4.7 8.38 8.38 0 0 1-3.8-.9L3 21l1.9-5.7a8.38 8.38 0 0 1-.9-3.8 8.5 8.5 0 0 1 4.7-7.6 8.38 8.38 0 0 1 3.8-.9h.5a8.48 8.48 0 0 1 8 8v.5z"/></svg>',
    file:
      '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/><line x1="12" y1="18" x2="12" y2="12"/><line x1="9" y1="15" x2="15" y2="15"/></svg>',
    folder:
      '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/></svg>',
    megaphone:
      '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 11l18-7v16L3 13z"/><path d="M11.6 16.8a3 3 0 1 1-5.8-1.6"/></svg>',
    gear:
      '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 1 1-4 0v-.09a1.65 1.65 0 0 0-1-1.51 1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 1 1 0-4h.09a1.65 1.65 0 0 0 1.51-1 1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33h.08a1.65 1.65 0 0 0 1-1.51V3a2 2 0 1 1 4 0v.09a1.65 1.65 0 0 0 1 1.51h.08a1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82v.08a1.65 1.65 0 0 0 1.51 1H21a2 2 0 1 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"/></svg>',
    search:
      '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="11" cy="11" r="7"/><line x1="21" y1="21" x2="16.65" y2="16.65"/></svg>',
    lock:
      '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="11" width="18" height="11" rx="2"/><path d="M7 11V7a5 5 0 0 1 10 0v4"/></svg>',
    unlock:
      '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="11" width="18" height="11" rx="2"/><path d="M7 11V7a5 5 0 0 1 9.9-1"/></svg>',
    clip:
      '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21.44 11.05l-9.19 9.19a6 6 0 0 1-8.49-8.49l9.19-9.19a4 4 0 0 1 5.66 5.66L9.41 17.41a2 2 0 0 1-2.83-2.83l8.49-8.49"/></svg>',
    transfer:
      '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="17 1 21 5 17 9"/><path d="M3 11V9a4 4 0 0 1 4-4h14"/><polyline points="7 23 3 19 7 15"/><path d="M21 13v2a4 4 0 0 1-4 4H3"/></svg>',
    back:
      '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><polyline points="15 18 9 12 15 6"/></svg>',
    close:
      '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>',
    min:
      '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round"><line x1="5" y1="12" x2="19" y2="12"/></svg>',
    send:
      '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="22" y1="2" x2="11" y2="13"/><polygon points="22 2 15 22 11 13 2 9 22 2"/></svg>',
    users:
      '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M17 21v-2a4 4 0 0 0-4-4H5a4 4 0 0 0-4 4v2"/><circle cx="9" cy="7" r="4"/><path d="M23 21v-2a4 4 0 0 0-3-3.87"/><path d="M16 3.13a4 4 0 0 1 0 7.75"/></svg>',
    chatList:
      '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M14 9V5a3 3 0 0 0-3-3l-4 9v9h11.3a2 2 0 0 0 2-1.7l1.2-7a2 2 0 0 0-2-2.3z"/><path d="M7 22H4a2 2 0 0 1-2-2v-7a2 2 0 0 1 2-2h3"/></svg>',
    me:
      '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2"/><circle cx="12" cy="7" r="4"/></svg>',
    download:
      '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><polyline points="7 10 12 15 17 10"/><line x1="12" y1="15" x2="12" y2="3"/></svg>',
    openFolder:
      '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/><line x1="12" y1="11" x2="12" y2="17"/><polyline points="9 14 12 11 15 14"/></svg>',
  };

  return {
    T,
    invoke,
    listen,
    unlisten,
    IS_MOBILE,
    $,
    $$,
    escapeHtml,
    fmtSize,
    fmtSpeed,
    fmtEta,
    fmtTime,
    fmtDay,
    setSound,
    playSound,
    toast,
    showModal,
    promptModal,
    fileOfferModal,
    showContextMenu,
    closeContextMenu,
    avatarColor,
    avatarBg,
    osLabel,
    fileIcon,
    ICONS,
  };
})();
