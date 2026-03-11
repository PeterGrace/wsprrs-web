commit := `git rev-parse HEAD`
shortcommit := `git rev-parse HEAD`
transport := "docker://"
registry := "r.gfpd.us"
image := "library/wsprrs-web"
tag := `git describe --tags|| echo dev`
cargo_version := `grep '^version = ' Cargo.toml | head -1 | cut -d'"' -f2`

all: make-image

deploy: make-image sync-kustomize kustomize

make-image:
  docker buildx build --push --platform linux/amd64 \
  -t {{registry}}/{{image}}:latest \
  -t {{registry}}/{{image}}:{{shortcommit}} \
  -t {{registry}}/{{image}}:{{commit}} \
  -t {{registry}}/{{image}}:{{tag}} \
  .

release-patch:
  cargo release --no-publish --no-verify patch --execute
release-minor:
  cargo release --no-publish --no-verify minor --execute
release-major:
  cargo release --no-publish --no-verify minor --execute

# Sync kustomize/kustomization.yaml newTag with the current Cargo.toml version.
sync-kustomize:
  sed -i 's/newTag: .*/newTag: v{{cargo_version}}/' kustomize/kustomization.yaml

kustomize:
  kubectl apply -k kustomize/
