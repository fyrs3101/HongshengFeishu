/* 鸿升飞书 · 主窗口逻辑（单窗口 SPA）
 * 桌面：左侧联系人 + 右侧聊天/欢迎；设置与传输为模态对话框
 * 移动：底部四 Tab + 全屏聊天覆盖层
 */
(function () {
  "use strict";

  const $ = (id) => document.getElementById(id);
  const IS_MOBILE = HSF.IS_MOBILE;

  let peers = [];
  let settings = null;
  let selfInfo = null;
  let unread = {}; // uid -> 未读数
  let openChatUid = null; // 当前聊天对象（桌面嵌入 / 移动覆盖层）
  let chatApi = null; // 当前聊天组件 API
  let activeTab = "tab-peers";
  let platform = "windows"; // 由 get_platform 纠正

  /* ---------- 启动 ---------- */
  async function boot() {
    // 移动端：把侧边栏移入"联系人" Tab 容器
    if (IS_MOBILE) {
      const bar = $("sideBar");
      const peersTab = $("tab-peers");
      if (bar && peersTab && bar.parentElement !== peersTab) {
        peersTab.appendChild(bar);
      }
    }
    bindUI();
    try {
      platform = await HSF.invoke("get_platform");
    } catch (e) {
      platform = /Mac/i.test(navigator.platform || "") ? "macos" : "windows";
    }
    document.body.classList.add("plat-" + platform);
    await HSF.invoke("start").catch((e) => HSF.toast("启动失败：" + e));
    // 移动端启动页（对标飞鸽传书）：后台发现已启动，用户点击"使用鸿升飞书"后进入主界面
    if (IS_MOBILE) {
      const splash = $("splash");
      const enter = $("splashEnter");
      if (splash && enter && !splash.classList.contains("hide")) {
        await new Promise((resolve) => {
          enter.onclick = () => {
            splash.classList.add("hide");
            resolve();
          };
        });
      }
    }
    await refreshSettings(); // 先取设置，其余依赖它
    await Promise.all([refreshSelf(), refreshPeers()]);
    subscribeEvents();
  }

  /* ---------- 数据 ---------- */
  async function refreshSettings() {
    settings = await HSF.invoke("get_settings");
    HSF.setSound(!!settings.sound);
    $("selfNick").textContent = settings.nick || "未命名";
    $("selfStatus").textContent =
      settings.status === "away" ? "离开" : settings.status === "invisible" ? "隐身" : "在线";
    $("selfStatus").className =
      "tag-status" +
      (settings.status === "away" ? " away" : settings.status === "invisible" ? " invisible" : "");
    $("selfAvatar").textContent = (settings.nick || "飞").trim().charAt(0);
  }

  async function refreshSelf() {
    selfInfo = await HSF.invoke("get_self");
    const sub = `${settings.group || "未分组"} · ${selfInfo.ip}`;
    $("selfSub").textContent = sub;
    $("stIp").textContent = selfInfo.ip;
    if (IS_MOBILE) {
      $("meNick").value = settings.nick || "";
      $("meGroup").value = settings.group || "";
      $("meSound").checked = !!settings.sound;
      $("meAutoAccept").checked = !!settings.auto_accept;
      $("meAutoStart").checked = !!settings.auto_start;
      // 个人信息卡（对标飞鸽传书）
      $("meAvatar").textContent = (settings.nick || "飞").trim().charAt(0);
      $("meNickShow").textContent = settings.nick || "未命名";
      $("meIpShow").textContent = `${selfInfo.ip} · ${settings.group || "未分组"}`;
      // 收件夹路径提示
      const inboxPath = $("mInboxPath");
      if (inboxPath)
        inboxPath.textContent = selfInfo.inbox
          ? selfInfo.inbox.length > 30
            ? "…" + selfInfo.inbox.slice(-28)
            : selfInfo.inbox
          : "收到的文件在这里";
      $("meInfo").innerHTML =
        `本机名：${HSF.escapeHtml(selfInfo.host)}<br>` +
        `IP 地址：${HSF.escapeHtml(selfInfo.ip)}<br>` +
        `传输端口：${selfInfo.port}<br>` +
        `收件目录：${HSF.escapeHtml(selfInfo.inbox)}`;
    }
  }

  async function refreshPeers() {
    peers = await HSF.invoke("list_peers");
    renderPeers();
    renderSessions();
    updateCounts();
  }

  /* ---------- 渲染 ---------- */
  function displayName(p) {
    const alias = settings && settings.aliases && settings.aliases[p.uid];
    if (alias && alias.trim()) return { name: alias.trim(), aliased: true };
    return { name: p.nick || p.host || "未知", aliased: false };
  }

  function renderPeers() {
    const kw = ($("searchInput").value || "").trim().toLowerCase();
    const list = $("peerList");
    list.innerHTML = "";
    const filtered = peers.filter((p) => {
      if (!kw) return true;
      const dn = displayName(p).name;
      return (
        dn.toLowerCase().includes(kw) ||
        (p.host || "").toLowerCase().includes(kw) ||
        (p.ip || "").includes(kw) ||
        (p.group || "").toLowerCase().includes(kw)
      );
    });
    filtered.sort((a, b) => {
      const ao = a.status === "invisible" ? 2 : 0;
      const bo = b.status === "invisible" ? 2 : 0;
      return ao - bo;
    });
    if (!filtered.length) {
      const emptyTip = peers.length
        ? "没有匹配的联系人"
        : "正在发现网段内的同事…<br>请确认对方已打开鸿升飞书且在同一网络";
      const diagBtn = peers.length
        ? ""
        : `<br><button class="diag-entry" id="netDiagBtn">发现不了？点此排查</button>`;
      list.innerHTML = `<li class="empty">${emptyTip}${diagBtn}</li>`;
      if (!peers.length && $("netDiagBtn")) {
        $("netDiagBtn").onclick = openNetDiag;
      }
      return;
    }
    for (const p of filtered) {
      const dn = displayName(p);
      const li = document.createElement("li");
      li.className =
        "peer" +
        (p.status === "invisible" ? " offline" : "") +
        (openChatUid === p.uid ? " on" : "");
      li.dataset.uid = p.uid;
      const offline = p.status === "invisible";
      li.innerHTML = `
        <div class="avatar" style="background:${HSF.avatarBg(p.uid)}">${HSF.escapeHtml(
        dn.name.charAt(0)
      )}<span class="dot ${offline ? "off" : ""}"></span></div>
        <div class="peer-meta">
          <div class="peer-name">${HSF.escapeHtml(dn.name)}${
            dn.aliased ? '<span class="alias-flag">备注</span>' : ""
          }</div>
          <div class="peer-sub"><span class="os-tag">${HSF.osLabel(p.os)}</span>${HSF.escapeHtml(
        p.host || ""
      )} · ${HSF.escapeHtml(p.ip)}${p.group ? " · " + HSF.escapeHtml(p.group) : ""}</div>
        </div>
        ${unread[p.uid] ? `<span class="unread">${unread[p.uid]}</span>` : ""}
      `;
      li.onclick = () => openChat(p.uid);
      li.oncontextmenu = (e) => {
        e.preventDefault();
        HSF.showContextMenu(e.clientX, e.clientY, [
          {
            label: "发消息",
            onClick: () => openChat(p.uid),
          },
          {
            label: dn.aliased ? "修改备注" : "添加备注",
            onClick: async () => {
              const v = await HSF.promptModal({
                title: "设置备注名",
                label: "备注名（留空取消）",
                value: dn.aliased ? dn.name : "",
                placeholder: "例如：周老师",
              });
              if (v === null) return;
              HSF.invoke("set_alias", { uid: p.uid, alias: v || "" }).then(refreshPeers);
            },
          },
          { label: "发送文件…", onClick: () => pickAndSend(p.uid) },
          {
            label: "发送文件夹…",
            onClick: () => pickAndSend(p.uid, true),
          },
        ]);
      };
      // 桌面端：拖文件到联系人上直接发送
      if (!IS_MOBILE) {
        li.addEventListener("dragover", (e) => {
          e.preventDefault();
          li.style.background = "var(--blue-soft)";
        });
        li.addEventListener("dragleave", () => (li.style.background = ""));
        li.addEventListener("drop", async (e) => {
          e.preventDefault();
          li.style.background = "";
          const files = Array.from(e.dataTransfer.files || []);
          for (const f of files) {
            const pth = f.path || f.webkitRelativePath;
            if (pth) {
              HSF.invoke("send_file", { peerUid: p.uid, path: pth }).catch((err) =>
                HSF.toast("发送失败：" + err)
              );
            }
          }
        });
      }
      list.appendChild(li);
    }
  }

  function renderSessions() {
    if (!IS_MOBILE) return;
    const list = $("sessionList");
    HSF.invoke("list_sessions").then((arr) => {
      list.innerHTML = "";
      if (!arr.length) {
        list.innerHTML = '<li class="empty">暂无会话，去联系人列表找同事聊聊吧</li>';
        return;
      }
      for (const s of arr) {
        const p = peers.find((x) => x.uid === s.uid);
        const dn = p ? displayName(p).name : "离线联系人";
        const li = document.createElement("li");
        li.className = "peer";
        const last = s.last;
        const preview =
          last.kind === "file"
            ? `[文件] ${last.file_name || ""}`
            : last.enc
            ? "[加密消息]"
            : last.text || "";
        li.innerHTML = `
          <div class="avatar" style="background:${HSF.avatarBg(s.uid)}">${HSF.escapeHtml(
          dn.charAt(0)
        )}<span class="dot"></span></div>
          <div class="peer-meta">
            <div class="peer-name">${HSF.escapeHtml(dn)}</div>
            <div class="peer-sub">${HSF.escapeHtml(preview.slice(0, 40))}</div>
          </div>
          <span style="font-size:11px;color:var(--muted);flex:none">${HSF.fmtTime(last.ts)}</span>
        `;
        li.onclick = () => openChat(s.uid);
        list.appendChild(li);
      }
    });
  }

  function updateCounts() {
    const online = peers.filter((p) => p.status !== "invisible").length;
    $("onlineCount").textContent = peers.length;
    $("stOnline").textContent = online;
    const total = Object.values(unread).reduce((a, b) => a + b, 0);
    const badge = $("tabBadge");
    if (badge) {
      badge.hidden = !total;
      badge.textContent = total > 99 ? "99+" : total;
    }
  }

  /* ---------- 打开聊天（桌面嵌入主区 / 移动全屏覆盖层） ---------- */
  function openChat(uid) {
    unread[uid] = 0;
    updateCounts();
    renderPeers();
    if (IS_MOBILE) {
      const ov = $("chatOverlay");
      if (openChatUid === uid && ov.classList.contains("on")) return;
      if (openChatUid) ov.innerHTML = "";
      openChatUid = uid;
      ov.classList.add("on");
      chatApi = window.HSFChat.createChat(ov, uid, {
        embedded: true,
        onClose: () => {
          ov.classList.remove("on");
          ov.innerHTML = "";
          openChatUid = null;
          chatApi = null;
        },
      });
    } else {
      if (openChatUid === uid) return;
      openChatUid = uid;
      $("welcome").hidden = true;
      const holder = $("chatHolder");
      holder.hidden = false;
      holder.innerHTML = "";
      chatApi = window.HSFChat.createChat(holder, uid, {
        embedded: true,
        onClose: () => {
          openChatUid = null;
          chatApi = null;
          holder.innerHTML = "";
          holder.hidden = true;
          $("welcome").hidden = false;
          renderPeers();
        },
      });
    }
  }

  /* ---------- 文件选择发送 ---------- */
  async function pickAndSend(uid, dir = false) {
    try {
      // 优先走 dialog 插件命令；失败时回退 Rust 兜底命令
      let selected = null;
      try {
        selected = dir
          ? await HSF.invoke("plugin:dialog|open", { options: { directory: true } })
          : await HSF.invoke("plugin:dialog|open", { options: { multiple: true } });
      } catch (pluginErr) {
        selected = dir
          ? await HSF.invoke("pick_directory")
          : await HSF.invoke("pick_files");
      }
      if (!selected) return;
      const paths = Array.isArray(selected) ? selected : [selected];
      for (const p of paths) {
        HSF.invoke("send_file", { peerUid: uid, path: p }).catch((e) => HSF.toast("发送失败：" + e));
      }
    } catch (e) {
      HSF.toast("无法打开文件选择器：" + (e && e.message ? e.message : e));
    }
  }

  /* ---------- 移动端文件 Tab：分类选择 → 选择联系人发送（对标飞鸽） ---------- */
  async function pickCategoryAndSend(cat) {
    // 选择文件：优先 dialog 插件，失败回退 Rust 兜底命令
    let selected = null;
    try {
      selected = await HSF.invoke("plugin:dialog|open", { options: { multiple: true } });
    } catch (pluginErr) {
      try {
        selected = await HSF.invoke("pick_files");
      } catch (e) {
        return HSF.toast("无法打开文件选择器：" + (e && e.message ? e.message : e));
      }
    }
    if (!selected) return;
    const paths = Array.isArray(selected) ? selected : [selected];
    if (!paths.length) return;
    // 选择发送对象：底部弹层列出在线联系人
    const online = peers.filter((p) => p.status !== "invisible");
    if (!online.length) return HSF.toast("当前没有在线联系人，请确认对方已打开鸿升飞书");
    if (online.length === 1) {
      sendPathsTo(online[0].uid, paths);
      return;
    }
    showPeerSheet(online, (uid) => sendPathsTo(uid, paths));
  }

  function sendPathsTo(uid, paths) {
    for (const p of paths) {
      HSF.invoke("send_file", { peerUid: uid, path: p }).catch((e) =>
        HSF.toast("发送失败：" + e)
      );
    }
    HSF.toast(`正在发送 ${paths.length} 个文件…`);
    switchTab("tab-files");
  }

  /* 底部联系人选择弹层（对标飞鸽：选完文件后选人） */
  function showPeerSheet(list, onPick) {
    closePeerSheet();
    const mask = document.createElement("div");
    mask.className = "sheet-mask";
    const sheet = document.createElement("div");
    sheet.className = "sheet";
    sheet.innerHTML = `
      <div class="sheet-head">
        <span>发送给</span>
        <button class="sheet-x" aria-label="关闭">✕</button>
      </div>
      <div class="sheet-body"></div>`;
    const body = sheet.querySelector(".sheet-body");
    for (const p of list) {
      const dn = displayName(p);
      const row = document.createElement("button");
      row.className = "sheet-peer";
      row.innerHTML = `
        <span class="avatar" style="background:${HSF.avatarBg(p.uid)}">${HSF.escapeHtml(
        dn.name.charAt(0)
      )}</span>
        <span class="sp-meta">
          <span class="sp-name">${HSF.escapeHtml(dn.name)}</span>
          <span class="sp-sub">${HSF.escapeHtml(p.host || "")} · ${HSF.escapeHtml(p.ip)}</span>
        </span>`;
      row.onclick = () => {
        closePeerSheet();
        onPick(p.uid);
      };
      body.appendChild(row);
    }
    mask.onclick = (e) => {
      if (e.target === mask) closePeerSheet();
    };
    sheet.querySelector(".sheet-x").onclick = closePeerSheet;
    mask.appendChild(sheet);
    document.body.appendChild(mask);
    requestAnimationFrame(() => mask.classList.add("on"));
  }

  function closePeerSheet() {
    document.querySelectorAll(".sheet-mask").forEach((m) => m.remove());
  }

  /* ---------- 群发 ---------- */
  async function groupSend() {
    const text = await HSF.promptModal({
      title: "群发消息",
      label: "将发送给所有在线联系人",
      placeholder: "输入群发内容",
    });
    if (!text || !text.trim()) return;
    const online = peers.filter((p) => p.status !== "invisible");
    if (!online.length) return HSF.toast("当前没有在线联系人");
    let ok = 0;
    for (const p of online) {
      try {
        await HSF.invoke("send_text", { peerUid: p.uid, text: text.trim(), pass: null });
        ok++;
      } catch (e) {}
    }
    HSF.toast(`已群发给 ${ok}/${online.length} 位联系人`);
  }

  /* ---------- 事件 ---------- */
  function subscribeEvents() {
    HSF.listen("peers", () => refreshPeers());

    HSF.listen("message", async (ev) => {
      const m = ev.payload;
      if (!m || m.direction !== "in") return;
      const uid = m.peer_uid;
      // 当前聊天对象正开着：由聊天组件自己处理，不累计未读
      if (openChatUid === uid) {
        if (activeTab === "tab-files") renderMobileTransfers();
        return;
      }
      unread[uid] = (unread[uid] || 0) + 1;
      HSF.playSound(m.kind === "file" ? "file" : "msg");
      updateCounts();
      renderPeers();
      renderSessions();
      if (activeTab === "tab-files") renderMobileTransfers();
      refreshTransfersModal();
    });

    // 文件接收确认（单窗口：统一由主窗口处理）
    HSF.listen("file-offer", (ev) => {
      const o = ev.payload || {};
      if (!o.transfer_id) return;
      handleOffer(o);
    });

    HSF.listen("settings-changed", () => {
      refreshSettings();
      refreshSelf();
      refreshPeers();
    });

    // 移动端：Rust 侧 open_chat_window 以事件形式通知前端
    HSF.listen("open-chat", (ev) => {
      if (ev.payload) openChat(ev.payload);
    });

    // 系统通知条（Windows 防火墙拦截提示等，Rust 侧 notice 事件）
    HSF.listen("notice", (ev) => {
      showNotice(ev.payload);
    });
  }

  /* ---------- 系统通知条 ---------- */
  function showNotice(n) {
    if (!n || !n.title) return;
    let bar = $("noticeBar");
    if (!bar) {
      bar = document.createElement("div");
      bar.id = "noticeBar";
      bar.className = "notice-bar";
      bar.hidden = true;
      document.body.appendChild(bar);
    }
    const isFw = n.kind === "firewall";
    bar.innerHTML = `
      <div class="nb-body">
        <div class="nb-title">${HSF.escapeHtml(n.title)}</div>
        <div class="nb-text">${HSF.escapeHtml(n.body || "")}</div>
      </div>
      <div class="nb-actions">
        ${
          isFw
            ? `<button class="nb-btn primary" id="nbFix">一键修复</button>
               <button class="nb-btn" id="nbGuide">手动指引</button>`
            : ""
        }
        <button class="nb-close" id="nbClose" title="不再提示">✕</button>
      </div>`;
    bar.hidden = false;
    const fix = $("nbFix");
    if (fix)
      fix.onclick = () => {
        fixFirewall();
        closeNotice();
      };
    const guide = $("nbGuide");
    if (guide)
      guide.onclick = () => {
        openFwSettings();
        closeNotice();
      };
    $("nbClose").onclick = () => {
      closeNotice();
      HSF.invoke("dismiss_firewall_notice").catch(() => {});
    };
  }
  function closeNotice() {
    const bar = $("noticeBar");
    if (bar) bar.hidden = true;
  }

  /* ---------- 网络诊断（"发现不了？"排查入口） ---------- */
  async function openNetDiag() {
    let d;
    try {
      d = await HSF.invoke("net_diag");
    } catch (e) {
      HSF.toast("诊断失败：" + e);
      return;
    }
    const rows = [
      ["本机 IP", HSF.escapeHtml(d.ip || "")],
      ["操作系统", HSF.escapeHtml(HSF.osLabel(d.os))],
      ["发现端口", `UDP ${d.udp_port}`],
      ["传输端口", `TCP ${d.tcp_port}`],
      ["发现监听", d.listener_ok ? "正常" : "异常（持续重试中）"],
      ["已发广播", `${d.udp_sent} 帧`],
      ["已收广播", `${d.udp_recv} 帧`],
      ["在线联系人", `${d.peers} 人`],
      ["防火墙放行", d.firewall_ok ? "已放行" : "未放行"],
      ["广播目标", (d.targets || []).map((t) => HSF.escapeHtml(t)).join("<br>")],
    ];
    const hints = (d.hints || [])
      .map((h) => `<li>${HSF.escapeHtml(h)}</li>`)
      .join("");
    const needFw = d.os === "windows" && !d.firewall_ok;
    const v = await HSF.showModal({
      title: "网络诊断",
      html:
        `<div class="diag-rows">` +
        rows
          .map(
            ([k, val]) =>
              `<div class="diag-row"><span class="diag-k">${HSF.escapeHtml(
                k
              )}</span><span class="diag-v">${val}</span></div>`
          )
          .join("") +
        `</div>` +
        (hints
          ? `<div class="diag-hints"><b>排查建议：</b><ul>${hints}</ul></div>`
          : ""),
      buttons: needFw
        ? [
            { text: "一键修复防火墙", primary: true, value: "fix" },
            { text: "手动放行指引", value: "guide" },
            { text: "关闭" },
          ]
        : [{ text: "知道了", primary: true }],
    });
    if (v === "fix") fixFirewall();
    else if (v === "guide") openFwSettings();
  }

  async function fixFirewall() {
    HSF.toast("请在弹出窗口点「是」授权（UAC）", "info", 4500);
    try {
      await HSF.invoke("fix_firewall");
      HSF.toast("修复脚本已启动，执行完成后请重试发现", "info", 5000);
    } catch (e) {
      HSF.toast("修复失败：" + e + "，请按手动指引操作", "err", 6000);
    }
  }

  function openFwSettings() {
    HSF.invoke("open_firewall_settings").catch(() => {});
  }

  async function handleOffer(o) {
    HSF.playSound("file");
    const v = await HSF.fileOfferModal(o);
    const accept = v === "accept";
    HSF.invoke("respond_offer", {
      transferId: o.transfer_id,
      accept,
      savePath: null,
    }).catch((e) => HSF.toast("操作失败：" + e));
  }

  /* ---------- 设置模态 ---------- */
  let stBase = null;
  let stStatus = "online";

  function openSettings() {
    stBase = settings;
    stStatus = (settings && settings.status) || "online";
    $("stNick").value = (settings && settings.nick) || "";
    $("stGroup").value = (settings && settings.group) || "";
    $("stSound").checked = !!(settings && settings.sound);
    $("stAutoAccept").checked = !!(settings && settings.auto_accept);
    $("stAutoStart").checked = !!(settings && settings.auto_start);
    $("stInbox").value = (settings && settings.inbox_dir) || "";
    document.querySelectorAll("#settingsModal .sp-item").forEach((x) => {
      x.classList.toggle("on", x.dataset.status === stStatus);
    });
    renderStAliases();
    $("settingsModal").hidden = false;
  }

  async function renderStAliases() {
    const box = $("stAliasList");
    box.innerHTML = "";
    let pl = [];
    try {
      pl = await HSF.invoke("list_peers");
    } catch (e) {}
    const aliases = (stBase && stBase.aliases) || {};
    const uids = new Set([...pl.map((p) => p.uid), ...Object.keys(aliases)]);
    if (!uids.size) {
      box.innerHTML = '<div class="hint">暂无可备注的联系人</div>';
      return;
    }
    for (const uid of uids) {
      const p = pl.find((x) => x.uid === uid);
      const row = document.createElement("div");
      row.className = "f-row";
      const label = p ? p.nick || p.host || "未知" : uid.slice(0, 12);
      row.innerHTML = `
        <input type="text" value="${HSF.escapeHtml(label)}" disabled style="flex:1;opacity:.65" />
        <input type="text" class="st-alias-input" data-uid="${HSF.escapeHtml(uid)}"
          value="${HSF.escapeHtml(aliases[uid] || "")}" placeholder="备注名" style="flex:1" />
      `;
      box.appendChild(row);
    }
  }

  async function saveSettings() {
    const next = {
      ...(stBase || {}),
      nick: ($("stNick").value || "").trim() || "鸿升周老师",
      group: ($("stGroup").value || "").trim() || "鸿升组",
      status: stStatus,
      sound: $("stSound").checked,
      auto_accept: $("stAutoAccept").checked,
      auto_start: $("stAutoStart").checked,
      inbox_dir: ($("stInbox").value || "").trim() || null,
    };
    try {
      await HSF.invoke("save_settings", { settings: next });
      const inputs = document.querySelectorAll(".st-alias-input");
      for (const inp of inputs) {
        const v = inp.value.trim();
        const old = ((stBase && stBase.aliases) || {})[inp.dataset.uid] || "";
        if (v !== old) {
          await HSF.invoke("set_alias", { uid: inp.dataset.uid, alias: v });
        }
      }
      $("settingsModal").hidden = true;
      HSF.toast("设置已保存");
    } catch (e) {
      HSF.toast("保存失败：" + e);
    }
  }

  /* ---------- 传输管理模态 ---------- */
  function openTransfers() {
    renderTransfersModal();
    $("transfersModal").hidden = false;
  }

  function refreshTransfersModal() {
    if (!$("transfersModal").hidden) renderTransfersModal();
  }

  function renderTransfersModal() {
    const box = $("dTransfersBody");
    HSF.invoke("list_transfers").then((list) => {
      renderTransferList(box, list);
    });
  }

  function renderTransferList(box, list) {
    box.innerHTML = "";
    if (!list.length) {
      box.innerHTML = '<div class="tr-empty">暂无传输记录</div>';
      return;
    }
    for (const m of list) {
      const p = peers.find((x) => x.uid === m.peer_uid);
      const dn = p ? displayName(p).name : m.direction === "out" ? "发送" : "接收";
      const st =
        m.status === "received"
          ? "已完成"
          : m.status === "sending" || m.status === "receiving"
          ? `进行中 ${m.progress}%`
          : m.status === "rejected"
          ? "已拒绝"
          : m.status === "failed"
          ? "失败"
          : m.status;
      const cls =
        m.status === "received"
          ? "done"
          : m.status === "sending" || m.status === "receiving"
          ? "doing"
          : m.status === "rejected" || m.status === "failed"
          ? "bad"
          : "";
      const div = document.createElement("div");
      div.className = "tr-item";
      div.innerHTML = `
        <div class="fi">${m.is_dir ? "📁" : HSF.fileIcon(m.file_name || "")[0]}</div>
        <div class="tm">
          <div class="tname">${HSF.escapeHtml(m.file_name || "未命名文件")}</div>
          <div class="tsub">${m.direction === "out" ? "发送给" : "来自"} ${HSF.escapeHtml(
        dn
      )} · ${HSF.fmtSize(m.file_size)} · ${HSF.fmtTime(m.ts)}</div>
          ${
            m.status === "sending" || m.status === "receiving"
              ? `<div class="progress"><i style="width:${m.progress}%"></i></div>`
              : ""
          }
        </div>
        <div class="tstate ${cls}">${st}</div>
      `;
      box.appendChild(div);
    }
  }

  /* ---------- UI 绑定 ---------- */
  function bindUI() {
    // 交通灯（Windows/Linux 显示；macOS 用系统原生，CSS 隐藏）
    const tlClose = $("tlClose");
    const tlMin = $("tlMin");
    const tlMax = $("tlMax");
    if (tlClose) tlClose.onclick = () => HSF.invoke("win_close").catch(() => {});
    if (tlMin) tlMin.onclick = () => HSF.invoke("win_minimize").catch(() => {});
    if (tlMax)
      tlMax.onclick = async () => {
        try {
          const w = HSF.T.window.getCurrent();
          const max = await w.isMaximized();
          if (max) {
            await w.unmaximize();
            document.body.classList.remove("win-max");
          } else {
            await w.maximize();
            document.body.classList.add("win-max");
          }
        } catch (e) {}
      };

    $("settingsBtn").onclick = openSettings;
    $("transfersBtn").onclick = openTransfers;
    $("groupSendBtn").onclick = groupSend;
    $("refreshBtn").onclick = () => {
      HSF.invoke("refresh_peers").then(refreshPeers);
    };
    const rb2 = $("refreshBtn2");
    if (rb2)
      rb2.onclick = () => {
        HSF.invoke("refresh_peers").then(refreshPeers);
      };
    $("searchInput").addEventListener("input", renderPeers);

    // 设置模态
    $("settingsClose").onclick = () => ($("settingsModal").hidden = true);
    $("stCancel").onclick = () => ($("settingsModal").hidden = true);
    $("stSave").onclick = saveSettings;
    $("settingsModal").addEventListener("mousedown", (e) => {
      if (e.target === $("settingsModal")) $("settingsModal").hidden = true;
    });
    document.querySelectorAll("#settingsModal .sp-item").forEach((it) => {
      it.onclick = () => {
        document.querySelectorAll("#settingsModal .sp-item").forEach((x) => x.classList.remove("on"));
        it.classList.add("on");
        stStatus = it.dataset.status;
      };
    });
    $("stInboxPick").onclick = async () => {
      try {
        // 优先插件命令，失败回退 Rust 兜底命令
        let dir = null;
        try {
          dir = await HSF.invoke("plugin:dialog|open", { options: { directory: true } });
        } catch (pluginErr) {
          dir = await HSF.invoke("pick_directory");
        }
        if (dir) $("stInbox").value = dir;
      } catch (e) {
        HSF.toast("无法打开目录选择器：" + (e && e.message ? e.message : e));
      }
    };

    // 传输模态
    $("transfersClose").onclick = () => ($("transfersModal").hidden = true);
    $("transfersOk").onclick = () => ($("transfersModal").hidden = true);
    $("transfersClear").onclick = () => {
      $("dTransfersBody").innerHTML = '<div class="tr-empty">暂无传输记录</div>';
    };
    $("transfersModal").addEventListener("mousedown", (e) => {
      if (e.target === $("transfersModal")) $("transfersModal").hidden = true;
    });

    // 移动端
    const mRefresh = $("mRefresh");
    if (mRefresh)
      mRefresh.onclick = () => HSF.invoke("refresh_peers").then(refreshPeers);

    // 移动端头部齿轮：打开设置（复用桌面设置模态）
    const mSettings = $("mSettings");
    if (mSettings) mSettings.onclick = openSettings;

    // 收件夹：尝试用系统默认程序打开（桌面有效；安卓提示路径）
    const mInbox = $("mInboxBtn");
    if (mInbox)
      mInbox.onclick = async () => {
        const dir = (selfInfo && selfInfo.inbox) || "";
        if (!dir) return HSF.toast("收件目录不可用");
        try {
          await HSF.invoke("open_path", { path: dir, reveal: true });
        } catch (e) {
          HSF.toast("无法打开收件夹：" + (e && e.message ? e.message : e));
        }
      };

    // 文件 Tab：分类选择文件 → 底部弹层选择在线联系人发送
    document.querySelectorAll(".fg-file-cat").forEach((btn) => {
      btn.onclick = () => pickCategoryAndSend(btn.dataset.cat);
    });

    document.querySelectorAll(".tabbar .tb-item").forEach((btn) => {
      btn.onclick = () => switchTab(btn.dataset.tab);
    });

    // 移动端"我的"页
    const meSave = $("meSaveAll");
    if (meSave) meSave.onclick = saveMeSettings;
    const meNickSave = $("meNickSave");
    if (meNickSave) meNickSave.onclick = saveMeSettings;

    // Esc 关闭模态
    document.addEventListener("keydown", (e) => {
      if (e.key === "Escape") {
        $("settingsModal").hidden = true;
        $("transfersModal").hidden = true;
      }
    });
  }

  function switchTab(tab) {
    activeTab = tab;
    document.querySelectorAll(".tabbar .tb-item").forEach((b) => {
      b.classList.toggle("on", b.dataset.tab === tab);
    });
    document.querySelectorAll(".tab-page").forEach((p) => {
      p.classList.toggle("on", p.id === tab);
    });
    if (tab === "tab-sessions") renderSessions();
    if (tab === "tab-me") refreshSelf();
    if (tab === "tab-files") renderMobileTransfers();
  }

  function renderMobileTransfers() {
    const box = $("mTransfersBody");
    if (!box) return;
    HSF.invoke("list_transfers").then((list) => renderTransferList(box, list));
  }

  async function saveMeSettings() {
    const next = {
      ...settings,
      nick: ($("meNick").value || "").trim() || settings.nick,
      group: ($("meGroup").value || "").trim() || settings.group,
      sound: $("meSound").checked,
      auto_accept: $("meAutoAccept").checked,
      auto_start: $("meAutoStart").checked,
    };
    try {
      await HSF.invoke("save_settings", { settings: next });
      HSF.toast("设置已保存");
      refreshSettings();
      refreshSelf();
    } catch (e) {
      HSF.toast("保存失败：" + e);
    }
  }

  boot();
})();
