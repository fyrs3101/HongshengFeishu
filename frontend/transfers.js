/* 鸿升飞书 · 传输管理窗口逻辑 */
(function () {
  "use strict";

  const $ = (id) => document.getElementById(id);
  let peers = [];

  $("tlClose").onclick = () => HSF.invoke("win_close").catch(() => {});
  $("tlMin").onclick = () => HSF.invoke("win_minimize").catch(() => {});
  $("trRefresh").onclick = load;

  async function load() {
    let list = [];
    try {
      list = await HSF.invoke("list_transfers");
    } catch (e) {}
    try {
      peers = await HSF.invoke("list_peers");
    } catch (e) {
      peers = [];
    }
    render(list);
  }

  function nameOf(uid) {
    const p = peers.find((x) => x.uid === uid);
    return p ? p.nick || p.host || "未知" : "未知";
  }

  function render(list) {
    const box = $("trBody");
    const doing = list.filter((m) => m.status === "sending" || m.status === "receiving").length;
    $("trSummary").textContent = list.length ? `共 ${list.length} 条${doing ? ` · ${doing} 个进行中` : ""}` : "";
    if (!list.length) {
      box.innerHTML = '<div class="tr-empty">暂无传输记录</div>';
      return;
    }
    box.innerHTML = "";
    for (const m of list) {
      const div = document.createElement("div");
      div.className = "tr-item";
      div.dataset.transfer = m.transfer_id;
      const st =
        m.status === "received"
          ? "已完成"
          : m.status === "sending"
          ? "发送中"
          : m.status === "receiving"
          ? "接收中"
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
      div.innerHTML = `
        <div class="fi">${m.is_dir ? "📁" : HSF.fileIcon(m.file_name || "")[0]}</div>
        <div class="tm">
          <div class="tname">${HSF.escapeHtml(m.file_name || "未命名文件")}</div>
          <div class="tsub">${m.direction === "out" ? "发送给" : "来自"} ${HSF.escapeHtml(
        nameOf(m.peer_uid)
      )} · ${HSF.fmtSize(m.file_size)} · ${HSF.fmtTime(m.ts)}</div>
          ${
            m.status === "sending" || m.status === "receiving"
              ? `<div class="progress"><i style="width:${m.progress}%"></i></div>
                 <div class="progress-line"><span>${m.progress}%</span><span>${HSF.fmtSpeed(
                  m.speed_bps
                )}${m.eta_secs ? " · " + HSF.fmtEta(m.eta_secs) : ""}</span></div>`
              : ""
          }
        </div>
        <div class="tstate ${cls}">${st}</div>
      `;
      if (m.status === "received" && m.saved_path) {
        const btn = document.createElement("button");
        btn.className = "btn";
        btn.textContent = "打开";
        btn.style.flex = "none";
        btn.onclick = () =>
          HSF.invoke("open_path", { path: m.saved_path, reveal: false }).catch((e) =>
            HSF.toast("打开失败：" + e)
          );
        div.appendChild(btn);
      }
      box.appendChild(div);
    }
  }

  // 实时刷新
  HSF.listen("message", (ev) => {
    const m = ev.payload;
    if (!m || m.kind !== "file") return;
    const row = document.querySelector(`[data-transfer="${m.transfer_id}"]`);
    if (row) {
      const bar = row.querySelector(".progress > i");
      if (bar) bar.style.width = m.progress + "%";
      const st = row.querySelector(".tstate");
      if (st) {
        const map = {
          sending: "发送中",
          receiving: "接收中",
          received: "已完成",
          rejected: "已拒绝",
          failed: "失败",
        };
        st.textContent = map[m.status] || m.status;
        st.className =
          "tstate " +
          (m.status === "received" ? "done" : m.status === "sending" || m.status === "receiving" ? "doing" : "bad");
      }
    } else {
      load();
    }
  });

  load();
})();
