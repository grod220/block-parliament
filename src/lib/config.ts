export const config = {
  name: "Block Parliament",
  tagline: "Anza core dev validator",

  // Pubkeys - everything else is fetched from APIs using these
  identity: "mD1afZhSisoXfJLT8nYwSFANqjr1KPoDUEpYTEfFX1e",
  voteAccount: "4PL2ZFoZJHgkbZ54US4qNC58X69Fa1FKtY4CaVKeuQPg",

  // Contact
  contact: {
    twitter: "grod220",
  },

  // Links
  links: {
    validatorsApp:
      "https://www.validators.app/validators/mD1afZhSisoXfJLT8nYwSFANqjr1KPoDUEpYTEfFX1e?locale=en&network=mainnet",
    stakewiz: "https://stakewiz.com/validator/4PL2ZFoZJHgkbZ54US4qNC58X69Fa1FKtY4CaVKeuQPg",
    solscan: "https://solscan.io/account/4PL2ZFoZJHgkbZ54US4qNC58X69Fa1FKtY4CaVKeuQPg",
    sfdp: "https://solana.org/sfdp-validators/mD1afZhSisoXfJLT8nYwSFANqjr1KPoDUEpYTEfFX1e",
    jito: "https://www.jito.network/stakenet/steward/4PL2ZFoZJHgkbZ54US4qNC58X69Fa1FKtY4CaVKeuQPg/",
    ibrl: "https://ibrl.wtf/validator/mD1afZhSisoXfJLT8nYwSFANqjr1KPoDUEpYTEfFX1e/",
  },

  // Changelog entries - newest first
  changelog: [
    { date: "2026-01-13", event: "Site launch" },
    { date: "2026-01-10", event: "Upgraded to jito-BAM v3.0.14" },
    { date: "2026-01-01", event: "First MEV rewards earned (epoch 904)" },
    { date: "2025-12-30", event: "Received Solana Foundation delegation (epoch 903)" },
    { date: "2025-12-23", event: "Upgraded to jito v3.0.13" },
    { date: "2025-12-22", event: "First epoch with stake (epoch 899)" },
    { date: "2025-12-16", event: "Accepted into Solana Foundation Delegation Program (epoch 896)" },
    { date: "2025-11-19", event: "Bootstrapped validator with Agave client" },
  ],
} as const;

export type Config = typeof config;
