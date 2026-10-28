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
COPY --from=build /app/dist /usr/share/nginx/html
