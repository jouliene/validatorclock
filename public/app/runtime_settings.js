function setAddressType(type) {
  if ((type !== "ever" && type !== "ton") || !state.selectedChainId) {
    return;
  }
  state.addressTypes[state.selectedChainId] = type;
  try {
    window.localStorage?.setItem(ADDRESS_TYPE_KEY, JSON.stringify(state.addressTypes));
  } catch (error) {
    // The preference is optional; private browsing can reject storage writes.
  }
  state.roundRenderKey = null;
  renderNow();
}

function setSourceDisplayMode(mode) {
  if ((mode !== "meta" && mode !== "addr") || state.selectedChainId !== "ton") {
    return;
  }
  state.sourceDisplayModes[state.selectedChainId] = mode;
  try {
    window.localStorage?.setItem(SOURCE_DISPLAY_KEY, JSON.stringify(state.sourceDisplayModes));
  } catch (error) {
    // The preference is optional; private browsing can reject storage writes.
  }
  state.roundRenderKey = null;
  renderNow();
}
