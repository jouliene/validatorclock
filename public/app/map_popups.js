function locationPopupHtml(properties) {
  let nodes = [];

  try {
    nodes = JSON.parse(properties.nodes_json || "[]");
  } catch (error) {
    nodes = [];
  }

  const nodeCount = Number(properties.node_count || nodes.length || 0);

  return `
    <div class="popup-title">${escapeHtml(properties.city)}, ${escapeHtml(properties.country)}</div>
    <div class="popup-muted">${nodeCount} validator${nodeCount === 1 ? "" : "s"} at this location</div>
    ${nodeTableHtml(nodes)}
  `;
}

function clusterPopupHtml(clusterPointCount, totalNodeCount) {
  return `
    <div class="popup-title">Node cluster</div>
    <div class="popup-muted">${clusterPointCount} locations</div>
    <div class="popup-node-row">
      <div class="popup-ip">${totalNodeCount} total nodes</div>
      <div class="popup-isp">Cluster</div>
      <div class="popup-peer">Click to zoom in</div>
    </div>
  `;
}

function clusterLeavesPopupHtml(clusterPointCount, totalNodeCount, leaves) {
  const nodes = nodesFromClusterLeaves(leaves);
  return `
    <div class="popup-title">Node cluster</div>
    <div class="popup-muted">${totalNodeCount} validators / ${clusterPointCount} locations</div>
    ${nodeTableHtml(nodes)}
  `;
}

function nodesFromClusterLeaves(leaves) {
  return (Array.isArray(leaves) ? leaves : []).flatMap((leaf) => {
    try {
      return JSON.parse(leaf?.properties?.nodes_json || "[]");
    } catch (error) {
      return [];
    }
  });
}

function nodeTableHtml(nodes) {
  const safeNodes = Array.isArray(nodes) ? nodes : [];
  if (!safeNodes.length) {
    return "";
  }

  return `
    <div class="popup-node-list">
      <table class="popup-node-table">
        <colgroup>
          <col class="popup-col-ip">
          <col class="popup-col-isp">
          <col class="popup-col-peer">
        </colgroup>
        <thead>
          <tr>
            <th scope="col">IP</th>
            <th scope="col">ISP</th>
            <th scope="col">Validator pubkey</th>
          </tr>
        </thead>
        <tbody>
          ${safeNodes.map((node) => `
          <tr>
            <td class="popup-ip">${escapeHtml(node.ip)}</td>
            <td class="popup-isp">${escapeHtml(node.isp)}</td>
            <td class="popup-peer-cell">
              <code class="popup-peer" title="${escapeHtml(node.peer)}">${escapeHtml(node.peer)}</code>
            </td>
          </tr>
          `).join("")}
        </tbody>
      </table>
    </div>
  `;
}

function escapeHtml(value) {
  return String(value ?? "").replace(/[&<>"']/g, (char) => ({
    "&": "&amp;",
    "<": "&lt;",
    ">": "&gt;",
    "\"": "&quot;",
    "'": "&#39;"
  })[char]);
}
