function validatorSourceKind(validator, options = {}) {
  const source = validator && validator.source;
  if (source && source.address) {
    return "detail";
  }
  if (isDirectValidatorContract(validator)) {
    return "direct";
  }
  if (tonValidatorContractHash(validator, options)) {
    return "detail";
  }
  return "unknown";
}

function shouldDisplayTonSourceMetadata(validator, options = {}) {
  return options.chainId === "ton" && options.sourceDisplayMode !== "addr" && Boolean(validator?.source?.address);
}

function isDirectValidatorContract(validator) {
  return (
    validator?.contract_type === "EverWallet"
    || validator?.contract_type === "TonWalletV1R3"
    || validator?.contract_type === "TonVestingWallet"
  );
}

function tonValidatorContractHash(validator, options = {}) {
  if (options.chainId !== "ton") {
    return "";
  }
  if (validator?.contract_type && validator.contract_type !== "Unknown") {
    return "";
  }
  return validator?.contract_type_hash || "";
}

function shortenContractHash(hash) {
  return hash && hash.length > 12 ? `${hash.slice(0, 6)}...${hash.slice(-6)}` : (hash || "-");
}
