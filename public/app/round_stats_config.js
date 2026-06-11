const ROUND_STATS_CHARTS = [
  {
    key: "totalStake",
    title: "Total stake",
    unit: "stake",
    series: [{
      key: "total",
      label: "Total stake",
      value: (round) => roundStatsAmount(round.total_stake, round.total_stake_raw),
      tooltip: (round) => formatRoundStatsExactAmount(round.total_stake),
    }],
    latest: (round) => formatStakeAmount(round?.total_stake),
  },
  {
    key: "stakeRange",
    title: "Min/max stake",
    unit: "stake",
    series: [
      {
        key: "min",
        label: "Min stake",
        value: (round) => roundStatsAmount(round.min_stake, null),
        tooltip: (round) => formatRoundStatsExactAmount(round.min_stake),
      },
      {
        key: "max",
        label: "Max stake",
        value: (round) => roundStatsAmount(round.max_stake, null),
        tooltip: (round) => formatRoundStatsExactAmount(round.max_stake),
      },
    ],
    latest: (round) => {
      if (!round) {
        return "-";
      }
      return `${formatStakeAmount(round.min_stake)} / ${formatStakeAmount(round.max_stake)}`;
    },
  },
  {
    key: "validators",
    title: "Number of validators",
    unit: "count",
    series: [{
      key: "validators",
      label: "Validators",
      value: (round) => Number(round.validator_count),
      tooltip: (round) => formatWeight(round.validator_count || 0),
    }],
    latest: (round) => round?.validator_count ? formatWeight(round.validator_count) : "-",
  },
  {
    key: "profitability",
    title: "Profitability",
    unit: "percent",
    series: [{
      key: "profitability",
      label: "Profitability",
      value: (round) => Number(round.profitability_percent),
      tooltip: (round) => formatRoundStatsExactPercent(round.profitability_percent),
    }],
    latest: (round) => formatRoundStatsPercent(round?.profitability_percent),
  },
];
