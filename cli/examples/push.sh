oras push ghcr.io/kevredlabs/cypherpunk-demo:skelz-proof-1 \
  --artifact-type "application/vnd.skelz.proof.v1+json" \
  --annotation "org.opencontainers.artifact.created=$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
  --annotation "skelz.signature=eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9..." \
  --annotation "skelz.original-image=ghcr.io/kevredlabs/cypherpunk-demo:latest" \
  --annotation "skelz.original-digest=sha256:abcd1234..." \
  --annotation "skelz.tool=skelz-cli@v1.0.0" \
  signature.json