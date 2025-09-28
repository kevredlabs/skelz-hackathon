# ADR-0001: Architecture et choix techniques

## Contexte

Garantir l'intégrité des images déployées nécessite un registre de confiance, des attestations vérifiables et des politiques immuables.

## Décision

- Registre on-chain sur Solana (faible coût, finalité rapide)
- Attestations (SBOM, provenance) sur IPFS/Arweave
- Signatures via Sigstore/Cosign
- Webhook K8s (Admission Controller) pour l'enforcement

## Conséquences

- Transparence et immutabilité
- Dépendance à l'infra blockchain et IPFS
- Tooling multi-langages (Rust, Go, TS)
