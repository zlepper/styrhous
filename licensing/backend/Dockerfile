FROM mcr.microsoft.com/dotnet/sdk:10.0 AS build
WORKDIR /src
COPY LicensingOperationalMetrics.cs CompilerStyle.props .editorconfig ./
COPY backend/ ./backend/
COPY shared/ ./shared/
COPY infrastructure/Directory.Build.props infrastructure/Directory.Packages.props ./infrastructure/
COPY infrastructure/database-provisioning/ ./infrastructure/database-provisioning/
RUN dotnet publish infrastructure/database-provisioning/Styrhous.Licensing.DatabaseProvisioning.csproj -c Release -o /provisioning --no-self-contained
RUN dotnet publish backend/src/Styrhous.Licensing/Styrhous.Licensing.csproj -c Release -o /app --no-self-contained

FROM mcr.microsoft.com/dotnet/aspnet:10.0
WORKDIR /app
COPY --from=public.ecr.aws/awsguru/aws-lambda-adapter:1.0.1@sha256:5ade5099ba8bda7ef88726a24ad170c76eb295e4ee4fd46244063dca91f0e913 /lambda-adapter /opt/extensions/lambda-adapter
COPY --from=build /app .
COPY --from=build /provisioning ./database-provisioning/
ENV ASPNETCORE_URLS=http://0.0.0.0:8080 \
    AWS_LWA_PORT=8080 \
    AWS_LWA_READINESS_CHECK_PATH=/desktop/v1/openapi.json
ENTRYPOINT ["dotnet", "Styrhous.Licensing.dll"]
