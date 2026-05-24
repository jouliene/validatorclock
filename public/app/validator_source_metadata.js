function tonSourceMetadata(validator) {
  const sourceAddress = normalizeSourceAddress(validator?.source?.address);
  const explicit = TON_SOURCE_METADATA_BY_ADDRESS[sourceAddress];
  if (explicit) {
    return {
      className: sourceMetadataClass(explicit.label),
      ...explicit,
    };
  }

  if (validator?.contract_type === "WhalesPoolProxy") {
    return {
      label: "TON Whales",
      name: "TON Whales",
      detail: "Nominator pool source for a TON Whales masterchain proxy.",
      className: "whales",
    };
  }
  if (validator?.contract_type === "HipoValidatorProxy") {
    return {
      label: "Hipo",
      name: "Hipo Finance",
      detail: "Hipo liquid-staking treasury source.",
      className: "hipo",
    };
  }
  if (validator?.contract_type === "ValidatorController") {
    return {
      label: "LST Pool",
      name: "Liquid staking pool",
      detail: "tonstake_pool source. No public owner name was found for this address.",
      className: "lst",
    };
  }
  if (validator?.contract_type === "TonNominatorPool") {
    return {
      label: "Nominator",
      name: "Nominator pool validator",
      detail: "Validator address configured in a TON nominator pool.",
      className: "nominator",
    };
  }
  if (validator?.contract_type === "TonSingleNominatorPool") {
    return {
      label: "Owner",
      name: "Single nominator pool owner",
      detail: "Owner address configured in a TON single nominator pool.",
      className: "owner",
    };
  }
  if (validator?.contract_type === "SingleNominatorV1_0" || validator?.contract_type === "SingleNominatorV1_1") {
    return {
      label: "Owner",
      name: "Single nominator owner",
      detail: "Owner wallet configured in a TON Single Nominator contract.",
      className: "owner",
    };
  }
  if (isDirectValidatorContract(validator)) {
    return {
      label: "Direct",
      name: "Direct validation wallet",
      detail: "Validator wallet participates directly through Elector.",
      className: "direct",
    };
  }
  return {
    label: "Unknown",
    name: "Unknown source owner",
    detail: "No source metadata is known for this address yet.",
    className: "unknown",
  };
}

function normalizeSourceAddress(address) {
  return formatMasterchainAddress(address || "").toLowerCase();
}

function sourceMetadataClass(label) {
  return String(label || "unknown")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-|-$/g, "") || "unknown";
}
