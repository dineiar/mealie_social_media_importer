FROM node:24-alpine AS build
WORKDIR /app
COPY package.json package-lock.json ./
RUN --mount=type=cache,target=/root/.npm npm ci
COPY . .
RUN npm run build

FROM nginxinc/nginx-unprivileged:alpine
# The base entrypoint turns the container's nameservers into NGINX_LOCAL_RESOLVERS
# when NGINX_ENTRYPOINT_LOCAL_RESOLVERS is set. The filter keeps envsubst away
# from nginx's own $variables.
ENV BACKEND_URL=http://backend:8080 \
    NGINX_ENTRYPOINT_LOCAL_RESOLVERS=1 \
    NGINX_ENVSUBST_FILTER='^(BACKEND_URL|NGINX_LOCAL_RESOLVERS)$'
COPY nginx.conf /etc/nginx/templates/default.conf.template
# The stock default.conf goes, so a container that cannot write conf.d stops at
# the check below instead of serving it, with no proxy and no security headers.
RUN rm /etc/nginx/conf.d/default.conf
COPY --chmod=0755 <<'EOF' /docker-entrypoint.d/30-require-generated-config.sh
#!/bin/sh
[ -s /etc/nginx/conf.d/default.conf ] && exit 0
echo "ERROR: /etc/nginx/conf.d/default.conf was not generated, so nginx has no config. Run the container as uid 101 or with group 0, or make /etc/nginx/conf.d writable for its user (with read_only, mount a tmpfs there with mode=1777)." >&2
exit 1
EOF
COPY --from=build /app/dist /usr/share/nginx/html
