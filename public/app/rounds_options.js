function validatorRenderOptions(snapshot, extra = {}) {
  return {
    chainId: snapshot.chain.id,
    addressType: selectedAddressType(snapshot.chain.id),
    onAddressTypeChange: setAddressType,
    sourceDisplayMode: selectedSourceDisplayMode(snapshot.chain.id),
    onSourceDisplayModeChange: setSourceDisplayMode,
    glossaryLabels: validatorGlossaryLabelsForSnapshot(snapshot),
    mapNodesByPeer: state.validatorMapNodesByPeer,
    ...extra,
  };
}

function fakeValidatorTooltip() {
  return "Validator node IP not detected.";
}

function fakeValidatorPeerSet(round) {
  const peers = round && round.fake_validator_peers;
  if (!Array.isArray(peers)) {
    return null;
  }

  return new Set(
    peers
      .map((peer) => String(peer || "").toLowerCase())
      .filter(Boolean)
  );
}
