# Used only by scripts/check-dockerignore.sh: exports the build context after .dockerignore.
FROM scratch
COPY . /ctx
