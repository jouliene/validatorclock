// Node location statistics: the numbers behind the panel, no DOM here.
function buildNodeStats(nodes, validators, mapNodesByPeer) {
  const validatorsByPeer = new Map();
  let networkStake = 0;
  let networkWeightPercent = 0;
  let logicalMappedStake = 0;
  let logicalMappedWeightPercent = 0;
  let logicalMappedNodeCount = 0;
  const hasMapNodeLookup = typeof mapNodesByPeer?.has === "function";

  for (const validator of Array.isArray(validators) ? validators : []) {
    const peer = String(validator.public_key || "").toLowerCase();
    if (!peer) {
      continue;
    }
    const stake = nodeStatsNumericValue(validator.stake);
    const weightPercent = nodeStatsNumericValue(validator.weight_percent);
    validatorsByPeer.set(peer, { ...validator, stakeNumber: stake, weightPercentNumber: weightPercent });
    networkStake += stake;
    networkWeightPercent += weightPercent;
    const isMapped = validator?.map_node || (hasMapNodeLookup && mapNodesByPeer.has(peer));
    if (isMapped) {
      logicalMappedNodeCount += 1;
      logicalMappedStake += stake;
      logicalMappedWeightPercent += weightPercent;
    }
  }

  const mappedNodes = (Array.isArray(nodes) ? nodes : [])
    .map((node) => {
      const peer = String(node.peer || "").toLowerCase();
      const validator = validatorsByPeer.get(peer);
      const lat = Number(node.lat);
      const lon = Number(node.lon);
      return {
        ...node,
        peer,
        validator,
        stake: validator?.stakeNumber || 0,
        weightPercent: validator?.weightPercentNumber || 0,
        lat,
        lon,
      };
    })
    .filter((node) => node.peer && node.validator && Number.isFinite(node.lat) && Number.isFinite(node.lon));
  const uniqueMappedNodes = Array.from(
    mappedNodes.reduce((byPeer, node) => (byPeer.has(node.peer) ? byPeer : byPeer.set(node.peer, node)), new Map()).values(),
  );

  const countryRows = aggregateNodeStatsRows(uniqueMappedNodes, (node) => normalizeNodeStatsCountry(node.country), networkStake);
  const locationRows = aggregateNodeStatsRows(uniqueMappedNodes, (node) => nodeStatsLocationLabel(node), networkStake);
  const ispRows = aggregateNodeStatsRows(uniqueMappedNodes, (node) => String(node.isp || "Unknown").trim() || "Unknown", networkStake);
  const mappedLocationRows = nodeStatsMappedLocationCandidates(uniqueMappedNodes);
  const medoid = mappedLocationRows[0] || null;
  const currentSet = state.snapshot?.current_set || {};

  return {
    roundId: currentSet.round_id || "",
    roundColor: currentSet.round_color || "",
    networkValidators: validatorsByPeer.size,
    networkStake,
    networkWeightPercent,
    mappedNodes: logicalMappedNodeCount,
    mappedStake: logicalMappedStake,
    mappedStakePercent: networkStake ? (logicalMappedStake / networkStake) * 100 : 0,
    mappedWeightPercent: networkWeightPercent ? (logicalMappedWeightPercent / networkWeightPercent) * 100 : logicalMappedWeightPercent,
    countryRows,
    locationRows,
    ispRows,
    mappedLocationRows,
    medoid,
  };
}

function aggregateNodeStatsRows(nodes, labelForNode, networkStake) {
  const rows = new Map();
  for (const node of nodes) {
    const label = labelForNode(node) || "Unknown";
    if (!rows.has(label)) {
      rows.set(label, {
        label,
        nodes: 0,
        stake: 0,
        weightPercent: 0,
      });
    }
    const row = rows.get(label);
    row.nodes += 1;
    row.stake += node.stake;
    row.weightPercent += node.weightPercent;
  }

  return Array.from(rows.values())
    .map((row) => ({
      ...row,
      stakePercent: networkStake ? (row.stake / networkStake) * 100 : 0,
    }))
    .sort((left, right) => right.stake - left.stake || right.nodes - left.nodes || left.label.localeCompare(right.label));
}

function nodeStatsMappedLocationCandidates(nodes) {
  const locations = new Map();
  for (const node of nodes) {
    const label = nodeStatsLocationLabel(node);
    if (!locations.has(label)) {
      locations.set(label, {
        label,
        weightedLat: 0,
        weightedLon: 0,
        totalWeight: 0,
      });
    }
    const location = locations.get(label);
    const weight = nodeStatsDistanceWeight(node);
    location.weightedLat += node.lat * weight;
    location.weightedLon += node.lon * weight;
    location.totalWeight += weight;
  }

  return Array.from(locations.values())
    .filter((location) => location.totalWeight > 0)
    .map((location) => {
      const lat = location.weightedLat / location.totalWeight;
      const lon = location.weightedLon / location.totalWeight;
      return {
        label: location.label,
        ...nodeStatsDistanceForPoint(nodes, lat, lon),
      };
    })
    .sort((left, right) => left.weightedAverageKm - right.weightedAverageKm);
}

function nodeStatsDistanceForPoint(nodes, lat, lon) {
  const distances = [];
  let weightedTotal = 0;
  let totalWeight = 0;

  for (const node of nodes) {
    const distance = distanceBetweenCoordinatesKm(lat, lon, node.lat, node.lon);
    const weight = nodeStatsDistanceWeight(node);
    distances.push(distance);
    weightedTotal += distance * weight;
    totalWeight += weight;
  }

  distances.sort((left, right) => left - right);
  return {
    weightedAverageKm: totalWeight ? weightedTotal / totalWeight : 0,
    medianKm: nodeStatsPercentileFromSorted(distances, 0.5),
    p90Km: nodeStatsPercentileFromSorted(distances, 0.9),
  };
}

function nodeStatsDistanceWeight(node) {
  return node.stake > 0 ? node.stake : Math.max(node.weightPercent, 1);
}

function nodeStatsPercentileFromSorted(values, percentile) {
  if (!values.length) {
    return 0;
  }
  const index = Math.ceil(values.length * percentile) - 1;
  return values[Math.min(values.length - 1, Math.max(0, index))];
}

function nodeStatsLocationLabel(node) {
  const city = String(node.city || "").trim();
  const country = normalizeNodeStatsCountry(node.country);
  return city && country ? `${city}, ${country}` : city || country || "Unknown";
}

function normalizeNodeStatsCountry(value) {
  const country = String(value || "").trim();
  return country === "The Netherlands" ? "Netherlands" : country || "Unknown";
}

function nodeStatsNumericValue(value) {
  const number = Number(value || 0);
  return Number.isFinite(number) ? number : 0;
}

