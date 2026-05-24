async function loadTychoMapNodes() {
  if (tychoMapNodes && tychoMapNodesChainId === state.selectedChainId) {
    return tychoMapNodes;
  }

  return refreshTychoMapNodesForSnapshot();
}

async function refreshTychoMapNodesForSnapshot() {
  const chainId = state.selectedChainId;
  if (!mapAvailableForChain(chainId)) {
    state.tychoMappedPeers = null;
    state.tychoMapNodesByPeer = null;
    tychoMapNodes = null;
    tychoMapNodesChainId = null;
    return [];
  }

  try {
    const nodes = await fetchJson(`/api/chains/${encodeURIComponent(chainId)}/map`);
    tychoMapNodes = Array.isArray(nodes) ? nodes : [];
  } catch (error) {
    if (chainId === TYCHO_MAP_CHAIN_ID) {
      console.warn("Using bundled Tycho map nodes", error);
      tychoMapNodes = Array.isArray(window.TYCHO_NODES) ? window.TYCHO_NODES : [];
    } else {
      console.warn(`Unable to load ${chainId} map nodes`, error);
      tychoMapNodes = [];
    }
  }

  tychoMapNodesChainId = chainId;
  tychoMapNodes = filterTychoMapNodesToCurrentValidators(tychoMapNodes);
  state.tychoMappedPeers = tychoMappedPeerSet(tychoMapNodes);
  state.tychoMapNodesByPeer = tychoMapNodeMapByPeer(tychoMapNodes);
  updateTychoMapTitle();
  updateTychoMapSummary();
  refreshTychoMapSource();
  return tychoMapNodes;
}

function mapAvailableForChain(chainId) {
  return MAP_CHAIN_IDS.has(chainId);
}

function currentMapChain() {
  return state.chains.find((chain) => chain.id === state.selectedChainId) || null;
}

function currentMapChainName() {
  return currentMapChain()?.name || state.selectedChainId || "Validator";
}

function tychoMappedPeerSet(nodes) {
  return new Set(
    (Array.isArray(nodes) ? nodes : [])
      .map((node) => String(node.peer || "").toLowerCase())
      .filter(Boolean)
  );
}

function tychoMapNodeMapByPeer(nodes) {
  const byPeer = new Map();
  for (const node of Array.isArray(nodes) ? nodes : []) {
    const peer = String(node.peer || "").toLowerCase();
    if (peer) {
      byPeer.set(peer, node);
    }
  }
  return byPeer;
}

function filterTychoMapNodesToCurrentValidators(nodes) {
  const validators = state.snapshot?.current_set?.validators;
  if (!Array.isArray(nodes) || !Array.isArray(validators)) {
    return [];
  }

  const activePeers = new Set(
    validators
      .map((validator) => String(validator.public_key || "").toLowerCase())
      .filter(Boolean)
  );

  return nodes.filter((node) => activePeers.has(String(node.peer || "").toLowerCase()));
}

function tychoMapFeatures() {
  const rawNodes = tychoMapNodes || [];
  const locationGroups = groupNodesByLocation(rawNodes);
  return locationGroups.map((group) => ({
    type: "Feature",
    geometry: {
      type: "Point",
      coordinates: [group.lon, group.lat]
    },
    properties: {
      city: group.city,
      country: group.country,
      isp: group.isp,
      node_count: group.nodes.length,
      nodes_json: JSON.stringify(group.nodes)
    }
  }));
}

function groupNodesByLocation(nodes) {
  const groups = [];

  for (const node of nodes) {
    const lat = Number(node.lat);
    const lon = Number(node.lon);
    if (!Number.isFinite(lat) || !Number.isFinite(lon)) {
      continue;
    }

    const cityKey = normalizeLocationPart(node.city);
    const countryKey = normalizeLocationPart(node.country);
    const ispKey = normalizeLocationPart(node.isp);
    let group = findMatchingLocationGroup(groups, cityKey, countryKey, ispKey, lat, lon);

    if (!group) {
      group = {
        lat: 0,
        lon: 0,
        city: node.city,
        country: node.country,
        isp: node.isp,
        cityKey,
        countryKey,
        ispKeys: new Set(),
        nodes: []
      };
      groups.push(group);
    }

    if (ispKey) {
      group.ispKeys.add(ispKey);
    }
    group.lat += lat;
    group.lon += lon;
    group.nodes.push(node);
  }

  return groups.map(({ cityKey: _cityKey, countryKey: _countryKey, ispKeys: _ispKeys, ...group }) => ({
    ...group,
    lat: group.lat / Math.max(group.nodes.length, 1),
    lon: group.lon / Math.max(group.nodes.length, 1)
  }));
}

function findMatchingLocationGroup(groups, cityKey, countryKey, ispKey, lat, lon) {
  for (const group of groups) {
    if (group.cityKey !== cityKey || group.countryKey !== countryKey) {
      continue;
    }

    const distanceKm = distanceToLocationGroupKm(group, lat, lon);
    if (distanceKm <= TYCHO_MAP_CLOSE_LOCATION_RADIUS_KM) {
      return group;
    }

    if (
      cityKey &&
      countryKey &&
      ispKey &&
      group.ispKeys.has(ispKey) &&
      distanceKm <= TYCHO_MAP_PROVIDER_CITY_RADIUS_KM
    ) {
      return group;
    }
  }
  return null;
}

function distanceToLocationGroupKm(group, lat, lon) {
  let nearest = Infinity;
  for (const node of group.nodes) {
    const nodeLat = Number(node.lat);
    const nodeLon = Number(node.lon);
    if (!Number.isFinite(nodeLat) || !Number.isFinite(nodeLon)) {
      continue;
    }
    nearest = Math.min(nearest, distanceBetweenCoordinatesKm(lat, lon, nodeLat, nodeLon));
  }
  return nearest;
}

function distanceBetweenCoordinatesKm(latA, lonA, latB, lonB) {
  const deltaLat = degreesToRadians(latB - latA);
  const deltaLon = degreesToRadians(lonB - lonA);
  const startLat = degreesToRadians(latA);
  const endLat = degreesToRadians(latB);
  const haversine = Math.sin(deltaLat / 2) ** 2
    + Math.cos(startLat) * Math.cos(endLat) * Math.sin(deltaLon / 2) ** 2;

  return 2 * TYCHO_MAP_EARTH_RADIUS_KM * Math.asin(Math.min(1, Math.sqrt(haversine)));
}

function degreesToRadians(value) {
  return value * Math.PI / 180;
}

function normalizeLocationPart(value) {
  return String(value || "").trim().toLowerCase();
}
