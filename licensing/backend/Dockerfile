FROM node:22-alpine AS portal-build
WORKDIR /src/licensing/portal
COPY licensing/portal/package.json licensing/portal/package-lock.json ./
RUN npm ci
COPY licensing/portal/ ./
COPY assets/icons/ /src/assets/icons/
COPY crates/components/src/icons/ /src/crates/components/src/icons/
COPY crates/components/assets/fonts/ /src/crates/components/assets/fonts/
RUN npm run build

FROM mcr.microsoft.com/dotnet/sdk:10.0 AS build
WORKDIR /src
COPY licensing/LicensingOperationalMetrics.cs .
COPY licensing/CompilerStyle.props .
COPY licensing/.editorconfig .
COPY licensing/backend/ ./backend/
RUN dotnet publish backend/src/Styrhous.Licensing/Styrhous.Licensing.csproj -c Release -o /app --no-self-contained

FROM mcr.microsoft.com/dotnet/aspnet:10.0
WORKDIR /app
RUN apt-get update \
    && apt-get install --yes --no-install-recommends libgssapi-krb5-2 \
    && rm -rf /var/lib/apt/lists/*
COPY --from=build /app .
COPY --from=portal-build /src/licensing/portal/build/ ./wwwroot/
ENV ASPNETCORE_URLS=http://+:8080
ENTRYPOINT ["dotnet", "Styrhous.Licensing.dll"]
