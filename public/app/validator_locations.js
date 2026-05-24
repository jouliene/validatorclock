function withValidatorLocationTooltipLines(validator, tooltipLines, options = {}, missingLocationLine = "") {
  return [
    ...(Array.isArray(tooltipLines) ? tooltipLines : []),
    ...validatorLocationTooltipLines(validator, options, missingLocationLine),
  ];
}

function validatorLocationTooltipLines(validator, options = {}, missingLocationLine = "") {
  const node = validatorMapNode(validator, options);
  if (!node) {
    const line = String(missingLocationLine || "").trim();
    return line ? [missingLocationTooltipLine(line)] : [];
  }

  const lines = ["Location:"];
  const ip = String(node.ip || "").trim();
  const isp = String(node.isp || "").trim();
  const place = [node.city, node.country]
    .map((part) => String(part || "").trim())
    .filter(Boolean)
    .join(", ");

  if (ip) {
    lines.push(`IP: ${ip}`);
  }
  if (isp) {
    lines.push(`ISP: ${isp}`);
  }
  if (place) {
    lines.push(`Place: ${place}`);
  }

  if (lines.length > 1) {
    return lines;
  }
  const line = String(missingLocationLine || "").trim();
  return line ? [missingLocationTooltipLine(line)] : [];
}

function missingLocationTooltipLine(line) {
  if (line.startsWith(VALIDATOR_TOOLTIP_DANGER_PREFIX)) {
    const text = line.slice(VALIDATOR_TOOLTIP_DANGER_PREFIX.length).trim();
    return validatorTooltipDangerLine(`Location: ${text}`);
  }
  return `Location: ${line}`;
}

function validatorMapNode(validator, options = {}) {
  if (validator?.map_node && typeof validator.map_node === "object") {
    return validator.map_node;
  }

  const peer = String(validator?.public_key || "").toLowerCase();
  if (!peer || !(options.mapNodesByPeer instanceof Map)) {
    return null;
  }
  return options.mapNodesByPeer.get(peer) || null;
}

function validatorMapAvailableForChain(chainId) {
  if (typeof mapAvailableForChain === "function") {
    return mapAvailableForChain(chainId);
  }
  return chainId === BUNDLED_TYCHO_MAP_CHAIN_ID;
}
