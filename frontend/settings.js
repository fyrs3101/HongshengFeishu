/* 鸿升飞书 · 设置窗口逻辑 */
(function () {
  "use strict";

  const $ = (id) => document.getElementById(id);
  let base = null;
  let peers = [];
  let status = "online";

  $("tlClose").onclick = () => HSF.invoke("win_close").catch(() => {});
  $("tlMin").onclick = () => HSF.invoke("win_minimize").catch(() => {});
  $("btnCancel").onclick = () => HSF.invoke("win_close").catch(() => {});

  document.querySelectorAll(".sp-item").forEach((it) => {
    it.onclick = () => {
      document.querySelectorAll(".sp-item").forEach((x) => x.classList.remove("on"));
      it.classList.add("on");
      status = it.dataset.status;
    };
  });

  $("fInboxPick").onclick = async () => {
    try {
      const { open } = HSF.T.dialog;
      const dir = await open({ directory: true });
      if (dir) $("fInbox").value = dir;
    } catch (e) {
      HSF.toast("无法打开目录选择器");
    }
  };

  async function boot() {
    base = await HSF.invoke("get_settings");
    status = base.status || "online";
    $("fNick").value = base.nick || "";
    $("fGroup").value = base.group || "";
    $("fSound").checked = !!base.sound;
    $("fAutoAccept").checked = !!base.auto_accept;
    $("fAutoStart").checked = !!base.auto_start;
    $("fInbox").value = base.inbox_dir || "";
    document.querySelectorAll(".sp-item").forEach((x) => {
      x.classList.toggle("on", x.dataset.status === status);
    });
    try {
      peers = await HSF.invoke("list_peers");
    } catch (e) {
      peers = [];
    }
    renderAliases();
  }

  function renderAliases() {
    const box = $("aliasList");
    box.innerHTML = "";
    const aliases = (base && base.aliases) || {};
    const uids = new Set([...peers.map((p) => p.uid), ...Object.keys(aliases)]);
    if (!uids.size) {
      box.innerHTML = '<div class="hint">暂无可备注的联系人</div>';
      return;
    }
    for (const uid of uids) {
      const p = peers.find((x) => x.uid === uid);
      const row = document.createElement("div");
      row.className = "f-row";
      row.innerHTML = `
        <input type="text" value="${HSF.escapeHtml(
          p ? p.nick || p.host || "未知" : uid.slice(0, 12)
        )}" disabled style="flex: 1; opacity: 0.7" />
        <input type="text" class="alias-input" data-uid="${HSF.escapeHtml(uid)}"
          value="${HSF.escapeHtml(aliases[uid] || "")}" placeholder="备注名" style="flex: 1" />
      `;
      box.appendChild(row);
    }
  }

  $("btnSave").onclick = async () => {
    const next = {
      ...base,
      nick: ($("fNick").value || "").trim() || "鸿升周老师",
      group: ($("fGroup").value || "").trim() || "鸿升组",
      status,
      sound: $("fSound").checked,
      auto_accept: $("fAutoAccept").checked,
      auto_start: $("fAutoStart").checked,
      inbox_dir: ($("fInbox").value || "").trim() || null,
    };
    try {
      await HSF.invoke("save_settings", { settings: next });
      // 逐个保存备注
      const inputs = document.querySelectorAll(".alias-input");
      for (const inp of inputs) {
        const v = inp.value.trim();
        const old = ((base && base.aliases) || {})[inp.dataset.uid] || "";
        if (v !== old) {
          await HSF.invoke("set_alias", { uid: inp.dataset.uid, alias: v });
        }
      }
      HSF.toast("设置已保存");
      HSF.invoke("win_close").catch(() => {});
    } catch (e) {
      HSF.toast("保存失败：" + e);
    }
  };

  boot();
})();
