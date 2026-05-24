function wireValidatorMapInteractions() {
  wireValidatorMapCursor("node-points");
  wireValidatorMapCursor("clusters");

  validatorMap.on("click", "node-points", handleValidatorNodeClick);
  validatorMap.on("click", "clusters", handleValidatorClusterClick);
  validatorMap.on("contextmenu", "clusters", handleValidatorClusterContextMenu);
}

function wireValidatorMapCursor(layerId) {
  validatorMap.on("mouseenter", layerId, () => {
    validatorMap.getCanvas().style.cursor = "pointer";
  });

  validatorMap.on("mouseleave", layerId, () => {
    validatorMap.getCanvas().style.cursor = "grab";
  });
}

function handleValidatorNodeClick(event) {
  const feature = event.features[0];

  validatorMapPopup("720px")
    .setLngLat(feature.geometry.coordinates)
    .setHTML(locationPopupHtml(feature.properties))
    .addTo(validatorMap);
}

async function handleValidatorClusterClick(event) {
  event.preventDefault();

  const clusterFeatures = validatorMap.queryRenderedFeatures(event.point, {
    layers: ["clusters"]
  });

  if (!clusterFeatures.length) {
    return;
  }

  const cluster = clusterFeatures[0];
  const clusterId = cluster.properties.cluster_id;
  const source = validatorMap.getSource("nodes");
  const pointCount = Number(cluster.properties.point_count || 0);
  const totalNodes = Number(cluster.properties.total_nodes || pointCount);
  const leaves = await source.getClusterLeaves(clusterId, Math.max(pointCount, 1), 0);
  const bounds = boundsForFeatures(leaves);
  const zoom = Math.min(
    VALIDATOR_MAP_MAX_ZOOM,
    Math.max(validatorMap.getZoom() + 1.25, await source.getClusterExpansionZoom(clusterId))
  );

  closeValidatorMapPopups();
  if (bounds) {
    validatorMap.fitBounds(bounds, {
      padding: clusterFitPadding(),
      maxZoom: zoom,
      duration: 450
    });
  } else {
    validatorMap.easeTo({
      center: cluster.geometry.coordinates,
      zoom,
      duration: 450
    });
  }

  if (clusterLeavesAreTight(leaves)) {
    window.setTimeout(() => {
      validatorMapPopup("720px")
        .setLngLat(cluster.geometry.coordinates)
        .setHTML(clusterLeavesPopupHtml(pointCount, totalNodes, leaves))
        .addTo(validatorMap);
    }, 480);
  }
}

function handleValidatorClusterContextMenu(event) {
  event.preventDefault();

  const cluster = event.features[0];
  const pointCount = cluster.properties.point_count;
  const totalNodes = cluster.properties.total_nodes || pointCount;

  validatorMapPopup("420px")
    .setLngLat(cluster.geometry.coordinates)
    .setHTML(clusterPopupHtml(pointCount, totalNodes))
    .addTo(validatorMap);
}

function validatorMapPopup(maxWidth) {
  return trackValidatorMapPopup(new maplibregl.Popup({
    closeButton: true,
    closeOnClick: true,
    maxWidth
  }));
}
