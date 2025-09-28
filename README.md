Soldock – Registry décentralisé d'images container sur Solana

## Pitch

- **Problème**: La supply chain logicielle est fragile (dépendances/images compromises, registries centralisés corruptibles/censurables). Comment garantir qu’une image déployée est bien celle construite et validée ?
- **Solution**: Un registre on-chain (Solana) des digests et signatures d’images, attestations (SBOM, provenance) sur IPFS/Arweave, politiques immuables, vérification automatique côté K8s (Admission Controller).
- **Workflow**: Build CI → Cosign signe → Publication on-chain (digest, signatures, CIDs IPFS) → Admission Controller compare digest/signatures/policies → ALLOW/DENY.
- **Bénéfices**: Sécurité, transparence, conformité, interopérabilité (Sigstore/Cosign), résistance à la censure.

## Objectif MVP (hackathon)

- Smart contract Solana (registre des digests, signatures, CIDs, politiques)
- CLI `publish`/`verify`
- K8s Admission Controller (Go) pour ALLOW/DENY
- Démo E2E (KinD): build → sign → publish → deploy

## Arborescence prévue

```
contracts/              # Programme Solana (Anchor/Rust)
cli/                    # CLI publish/verify (TS ou Rust)
admission-controller/   # Webhook K8s (Go)
sdk/                    # Clients partagés (ts/ rust)
infra/                  # Manifests K8s, Helm, KinD
docs/                   # Docs, schémas, ADRs
examples/               # Flux E2E
scripts/                # Helpers build/sign/publish/e2e
test/                   # Tests intégration/e2e
```

## Conventions

- Branches courtes `feat/*`, `fix/*`, merges sur `main` (protégée)
- Commits: Conventional Commits (`feat:`, `fix:`, `chore:`…)
- Formatage/lints: Rust (`rustfmt`/`clippy`), Go (`gofmt`/`golangci-lint`), TS (`eslint`/`prettier`)
- Hooks: `pre-commit`

## Prérequis (dev)

- Docker, Cosign, Node 20+, pnpm, Go 1.22+, Rust/Cargo, Solana CLI, Kind/kubectl

## Démarrage rapide

```
make setup
make build
make e2e   # démo end-to-end (à venir)
```

## Licence

MIT — voir `LICENSE`.


