const VALIDATOR_HEADER_CLASSES = {
  "#": "validator-index",
  Type: "validator-source-type",
  Source: "validator-source",
  Validator: "validator-id",
  History: "validator-history",
  Stake: "validator-stake",
  Rewards: "validator-rewards",
  Weight: "validator-weight",
  Seen: "validator-seen",
};

const VALIDATOR_NUMBER_HEADERS = new Set(["Stake", "Rewards", "Weight", "Seen"]);
const VALIDATOR_ROUND_HEADERS = ["#", "Type", "Source", "Validator", "History", "Stake", "Rewards", "Weight"];
const VALIDATOR_ABSENT_HEADERS = ["#", "Type", "Source", "Validator", "History", "Seen"];

const UNKNOWN_VALIDATOR_TYPE = { label: "UNKNOWN", className: "unknown" };

const VALIDATOR_CONTRACT_TYPES = {
  EverWallet: { label: "EVER", className: "ever" },
  DePoolProxy: { label: "PROXY", className: "proxy" },
  StEverDePoolProxy: { label: "StPROXY", className: "stproxy" },
  SingleNominatorV1_1: { label: "SNOMv1.1", className: "snom" },
  SingleNominatorV1_0: { label: "SNOMv1.0", className: "snom" },
  TonSingleNominatorPool: { label: "SNPOOL", className: "snpool" },
  TonNominatorPool: { label: "NOMPOOL", className: "nompool" },
  ValidatorController: { label: "LSTCTRL", className: "lstctrl" },
  TonWalletV1R3: { label: "V1R3", className: "v1r3" },
  TonVestingWallet: { label: "VEST", className: "vest" },
  WhalesPoolProxy: { label: "WHALES", className: "whales" },
  HipoValidatorProxy: { label: "HIPO", className: "hipo" },
};

const VALIDATOR_SOURCE_TYPES = {
  "533adf8a5680849177b9f213f61c48dfd8d730597078670d2367a5eef77251fe": {
    label: "StDEPOOL",
    className: "stdepool",
  },
  "14e20e304f53e6da152eb95fffc993dbd28245a775d847eed043f7c78a503885": {
    label: "DEPOOL",
    className: "depool",
  },
};

const VALIDATOR_TYPE_GLOSSARY = [
  { label: "EVER", name: "Ever Wallet", description: "Default Broxus wallet for Tycho TVM networks. Can be deployed in the masterchain and used directly for validation." },
  { label: "DEPOOL", name: "DePool", description: "Staking pool contract where many users can stake into one shared pool. The pool participates in validation through a proxy contract deployed in the masterchain." },
  { label: "StDEPOOL", name: "Staked EVER DePool", description: "Specialized DePool that uses liquid-staking funds for validation. It validates through a masterchain proxy contract, the same way as a regular DePool." },
  { label: "SNOMv1.1", name: "Single Nominator v1.1", description: "TON validator contract with a cold owner and hot validator role." },
  { label: "SNOMv1.0", name: "Single Nominator v1.0", description: "TON validator contract with a cold owner and hot validator role." },
  { label: "SNPOOL", name: "TON Single Nominator Pool", description: "Single-owner TON staking pool. One owner funds the pool, while a validator/controller wallet operates validation through Elector." },
  { label: "NOMPOOL", name: "TON Nominator Pool", description: "Multi-user TON staking pool where nominators delegate stake to a validator. The pool participates in validation and distributes rewards by pool settings." },
  { label: "LSTCTRL", name: "TON Liquid Staking Controller", description: "Masterchain controller used by TON liquid-staking pools. It receives validator stake from a basechain tonstake_pool, participates in validation through Elector, and returns funds and rewards according to the pool protocol." },
  { label: "V1R3", name: "TON Wallet V1 R3", description: "Standard TON wallet contract. It stores seqno and public key, accepts signed external messages, and can be deployed in the masterchain for direct validation." },
  { label: "VEST", name: "TON Vesting Wallet", description: "TON vesting wallet that locks funds on a schedule while still allowing approved staking operations. It can validate directly from the masterchain when Elector staking is allowed." },
  { label: "WHALES", name: "TON Whales Pool Proxy", description: "Masterchain proxy used by Ton Whales nominator pools. The basechain pool stores user stakes, while this proxy represents the pool in validation and forwards messages between the pool and Elector." },
  { label: "HIPO", name: "Hipo Validator Proxy", description: "Masterchain proxy used by Hipo liquid staking. Hipo Treasury funds the validator proxy for a validation round, and the proxy submits stake to Elector." },
  { label: "UNKNOWN", name: "Unknown", description: "Contract type has not been identified yet." },
];

const TON_SOURCE_METADATA_BY_ADDRESS = {
  "-1:950057f559dddf5ddfa64210f0e6536c55d219bf6fc7d7285b7735c215f21ef6": {
    label: "CAT Val 1",
    name: "CAT Validator 1",
    detail: "Named TON validator wallet from TonAPI metadata.",
  },
  "0:8c397c43f9ff0b49659b5d0a302b1a93af7ccc63e5f5c0c4f25a9dc1f8b47ab3": {
    label: "Telegram",
    name: "Telegram",
    detail: "Named owner wallet from TonAPI metadata.",
  },
  "0:a45b17f28409229b78360e3290420f13e4fe20f90d7e2bf8c4ac6703259e22fa": {
    label: "Tonstakers",
    name: "Tonstakers",
    detail: "Named tonstake_pool liquid-staking pool from TonAPI metadata.",
  },
  "0:c3f99ec4f68ef9a820dc029bd797d21603e087db5ad2d88d4285b6aa041e37fa": {
    label: "arabswallet",
    name: "arabswallet.ton",
    detail: "Named owner wallet from TonAPI metadata.",
  },
  "0:b66d1924b5cf9901357f54af01d5d98ddca5d944a89b94c8b2bc55bf694ccd4f": {
    label: "blackmarket",
    name: "blackmarket-dot-tg.ton",
    detail: "Named owner wallet from TonAPI metadata.",
  },
  "0:ccae1e65877a165df4b0f8d3db832e87cddea9cca601f1b07d2df1ddd29ce6ff": {
    label: "thedns",
    name: "thedns-telegram.ton",
    detail: "Named multisig owner from TonAPI metadata.",
  },
  "0:8bc991cfe177bc7e9721433efa3befd199485a55cffd040a06c89af026b71bcf": {
    label: "Hipo",
    name: "Hipo Finance",
    detail: "Hipo Treasury source discovered from the validator proxy.",
  },
};
