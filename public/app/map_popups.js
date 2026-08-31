function locationPopupContent(properties) {
  let nodes = [];

  try {
    nodes = JSON.parse(properties.nodes_json || "[]");
  } catch (error) {
    nodes = [];
  }

  const nodeCount = Number(properties.node_count || nodes.length || 0);

  return fragment([
    el("div", { className: "popup-title", text: `${properties.city}, ${properties.country}` }),
    el("div", {
      className: "popup-muted",
      text: `${nodeCount} validator${nodeCount === 1 ? "" : "s"} at this location`,
    }),
    nodeTableElement(nodes),
  ]);
}

function clusterPopupContent(clusterPointCount, totalNodeCount) {
  return fragment([
    el("div", { className: "popup-title", text: "Node cluster" }),
    el("div", { className: "popup-muted", text: `${clusterPointCount} locations` }),
    el("div", "popup-node-row", [
      el("div", { className: "popup-ip", text: `${totalNodeCount} total nodes` }),
      el("div", { className: "popup-isp", text: "Cluster" }),
      el("div", { className: "popup-peer", text: "Click to zoom in" }),
    ]),
  ]);
}

function clusterLeavesPopupContent(clusterPointCount, totalNodeCount, leaves) {
  return fragment([
    el("div", { className: "popup-title", text: "Node cluster" }),
    el("div", {
      className: "popup-muted",
      text: `${totalNodeCount} validators / ${clusterPointCount} locations`,
    }),
    nodeTableElement(nodesFromClusterLeaves(leaves)),
  ]);
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

function nodeTableElement(nodes) {
  const safeNodes = Array.isArray(nodes) ? nodes : [];
  if (!safeNodes.length) {
    return null;
  }

  return el("div", "popup-node-list", [
    el("table", "popup-node-table", [
      el(
        "colgroup",
        {},
        ["ip", "isp", "row", "validator"].map((key) => el("col", `popup-col-${key}`)),
      ),
      el("thead", {}, [
        el(
          "tr",
          {},
          ["IP", "ISP", "Row", "Validator"].map((label) =>
            el("th", { text: label, attrs: { scope: "col" } }),
          ),
        ),
      ]),
      el(
        "tbody",
        {},
        safeNodes.map((node) =>
          el("tr", {}, [
            el("td", { className: "popup-ip", text: node.ip }),
            el("td", { className: "popup-isp", text: node.isp }),
            el("td", "popup-row-cell", [nodeValidatorRowButton(node)]),
            el("td", "popup-peer-cell", [nodeValidatorDetails(node)]),
          ]),
        ),
      ),
    ]),
  ]);
}

function nodeValidatorRowButton(node) {
  const peerKey = String(node?.peer || "").toLowerCase();
  const row = Number(node?.validator_row || 0);
  const rowLabel = row > 0 ? `#${row}` : "#-";

  return el("button", {
    className: "popup-row-link",
    text: rowLabel,
    attrs: { type: "button", "aria-label": `Focus validator row ${rowLabel}` },
    dataset: { peer: peerKey },
  });
}

function nodeValidatorDetails(node) {
  const peer = String(node?.peer || "");
  const address = nodeValidatorAddressDisplay(String(node?.validator_wallet || ""));

  return el("div", "popup-validator-details", [
    popupValidatorDetail("Pubkey", peer || "-"),
    popupValidatorDetail("Address", address),
  ]);
}

function popupValidatorDetail(label, value) {
  return el("div", "popup-validator-detail", [
    el("span", { className: "popup-validator-label", text: label }),
    el("code", { className: "popup-validator-value", text: value }),
  ]);
}

function nodeValidatorAddressDisplay(wallet) {
  const raw = String(wallet || "");
  if (!raw) {
    return "-";
  }

  const formatted = formatDisplayAddress(raw, {
    chainId: state.selectedChainId,
    addressType: selectedAddressType(state.selectedChainId),
  });
  return formatted.value || formatted.text || raw;
}
