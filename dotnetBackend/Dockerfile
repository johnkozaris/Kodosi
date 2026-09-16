FROM mcr.microsoft.com/dotnet/sdk:10.0.401@sha256:2fa828c68761b1b8c23d7662dc134421b9d3b59fe1425fdbc80804e390cdb24d AS build
WORKDIR /source
COPY dotnetBackend/ dotnetBackend/
RUN dotnet restore dotnetBackend/src/Kodosi.Host/Kodosi.Host.csproj --locked-mode
RUN dotnet publish dotnetBackend/src/Kodosi.Host/Kodosi.Host.csproj --configuration Release --no-restore --output /app/publish /p:UseAppHost=false

FROM mcr.microsoft.com/dotnet/aspnet:10.0.12@sha256:6a94333d37514e385650a3c81a55e5350b67253dbe136e9cf17e499c35606a8c AS runtime
WORKDIR /app
RUN apt-get update && apt-get install -y --no-install-recommends curl && rm -rf /var/lib/apt/lists/* \
    && install -d -m 0700 -o "$APP_UID" -g "$APP_UID" /home/app/.aspnet /home/app/.aspnet/DataProtection-Keys
COPY --from=build /app/publish/ ./
USER $APP_UID
EXPOSE 8080
ENTRYPOINT ["dotnet", "Kodosi.Host.dll"]
