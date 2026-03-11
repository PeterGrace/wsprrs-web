commit := `git rev-parse HEAD`
shortcommit := `git rev-parse HEAD`
transport := "docker://"
registry := "r.gfpd.us"
image := "library/wsprrs-web"
tag := `git describe --tags|| echo dev`

all: make-image

make-image:
  docker buildx build --push --platform linux/amd64 \
  -t {{registry}}/{{image}}:latest \
  -t {{registry}}/{{image}}:{{shortcommit}} \
  -t {{registry}}/{{image}}:{{commit}} \
  -t {{registry}}/{{image}}:{{tag}} \
  .

