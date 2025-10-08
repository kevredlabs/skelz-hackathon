oras attach \
  --artifact-type "application/vnd.skelz.proof.v1+json" \
  --annotation "org.opencontainers.artifact.created=$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
  --annotation "skelz.signature=eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9..." \
  --annotation "skelz.original-image=ghcr.io/kevredlabs/cypherpunk-demo@sha256:53847b1184f2aea29a72e072d39f0aef7ff6305c9672ae9803fcca40c188d234" \
  --annotation "skelz.tool=skelz-cli@v1.0.0" \
  ghcr.io/kevredlabs/cypherpunk-demo@sha256:53847b1184f2aea29a72e072d39f0aef7ff6305c9672ae9803fcca40c188d234 \
  signature.json
