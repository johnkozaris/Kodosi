using System.Reflection;
using System.Text.Json;
using Kodosi.Application;
using Kodosi.Domain;
using Kodosi.Host.Endpoints;
using Kodosi.Host.Serialization;
using Microsoft.AspNetCore.Http;

namespace Kodosi.HostTests;

public sealed class SessionKeyEndpointTests
{
    [Theory]
    [InlineData("")]
    [InlineData(",\n        \"signatureVersion\": null")]
    public void KeyBlobEntry_Rejects_NonExplicit_SignatureVersion(
        string signatureVersionProperty)
    {
        var options = new JsonSerializerOptions(JsonSerializerDefaults.Web);
        HostJsonSerializerOptions.Configure(options);
        var json = $$"""
            {
              "recipientDeviceId": "recipient-device",
              "encryptedSessionKey": "AQID",
              "senderDeviceId": "sender-device",
              "keyGeneration": 0,
              "issuedAtMs": 1,
              "signature": "BAUG"{{signatureVersionProperty}}
            }
            """;

        Assert.Throws<JsonException>(() =>
            JsonSerializer.Deserialize<SessionKeyEndpoints.KeyBlobEntry>(json, options));
    }

    [Theory]
    [InlineData(SessionKeyBlobSignatureDigest.LegacyVersion)]
    [InlineData(SessionKeyBlobSignatureDigest.CurrentVersion)]
    public void KeyBlobEntry_Accepts_Explicit_SignatureVersion(int signatureVersion)
    {
        var options = new JsonSerializerOptions(JsonSerializerDefaults.Web);
        HostJsonSerializerOptions.Configure(options);
        var json = $$"""
            {
              "recipientDeviceId": "recipient-device",
              "encryptedSessionKey": "AQID",
              "senderDeviceId": "sender-device",
              "keyGeneration": 0,
              "issuedAtMs": 1,
              "signature": "BAUG",
              "signatureVersion": {{signatureVersion}}
            }
            """;

        var entry = JsonSerializer.Deserialize<SessionKeyEndpoints.KeyBlobEntry>(json, options);

        Assert.NotNull(entry);
        Assert.Equal(signatureVersion, entry.SignatureVersion);
    }

    [Fact]
    public async Task ClaimNextGenerationAsync_Returns_Generation_Wire_Response()
    {
        var session = CreateSession();

        var result = await SessionKeyEndpoints.ClaimNextGenerationAsync(
            session.Id.Value,
            CreateClaimer(session),
            session.OwnerUserId,
            session.IncarnationId,
            session.CurrentKeyGeneration,
            TestContext.Current.CancellationToken);

        Assert.Equal(
            StatusCodes.Status200OK,
            Assert.IsAssignableFrom<IStatusCodeHttpResult>(result).StatusCode);
        var response = Assert.IsType<SessionKeyGenerationClaimResponse>(
            Assert.IsAssignableFrom<IValueHttpResult>(result).Value);
        Assert.Equal(SessionKeyGenerationClaimResponseState.Claimed, response.State);
        Assert.Equal(1, response.Generation);
    }

    [Fact]
    public async Task ClaimNextGenerationAsync_Returns_NotFound_For_NonOwner()
    {
        var session = CreateSession();

        var result = await SessionKeyEndpoints.ClaimNextGenerationAsync(
            session.Id.Value,
            CreateClaimer(session),
            UserId.New(),
            session.IncarnationId,
            session.CurrentKeyGeneration,
            TestContext.Current.CancellationToken);

        Assert.Equal(
            StatusCodes.Status404NotFound,
            Assert.IsAssignableFrom<IStatusCodeHttpResult>(result).StatusCode);
    }

    [Fact]
    public async Task ClaimNextGenerationAsync_Returns_Current_Generation_For_Stale_Expectation()
    {
        var session = CreateSession();
        typeof(Session)
            .GetProperty(
                nameof(Session.CurrentKeyGeneration),
                BindingFlags.Instance | BindingFlags.Public | BindingFlags.NonPublic)!
            .SetValue(session, 4);

        var result = await SessionKeyEndpoints.ClaimNextGenerationAsync(
            session.Id.Value,
            CreateClaimer(session),
            session.OwnerUserId,
            session.IncarnationId,
            expectedCurrentGeneration: 3,
            ct: TestContext.Current.CancellationToken);

        var response = Assert.IsType<SessionKeyGenerationClaimResponse>(
            Assert.IsAssignableFrom<IValueHttpResult>(result).Value);
        Assert.Equal(SessionKeyGenerationClaimResponseState.GenerationChanged, response.State);
        Assert.Equal(4, response.Generation);
        Assert.Equal(4, session.CurrentKeyGeneration);
    }

    [Fact]
    public async Task ClaimNextGenerationAsync_Preserves_Ended_Conflict()
    {
        var session = CreateSession();
        session.End();

        var error = await Assert.ThrowsAsync<InvalidStateException>(() =>
            SessionKeyEndpoints.ClaimNextGenerationAsync(
                session.Id.Value,
                CreateClaimer(session),
                session.OwnerUserId,
                session.IncarnationId,
                session.CurrentKeyGeneration,
                TestContext.Current.CancellationToken));

        Assert.Equal(
            "Session is ended; no further key rotations accepted.",
            error.Message);
    }

    [Fact]
    public async Task ClaimNextGenerationAsync_Maps_Exhaustion_To_Conflict()
    {
        var session = CreateSession();
        typeof(Session)
            .GetProperty(
                nameof(Session.CurrentKeyGeneration),
                BindingFlags.Instance | BindingFlags.Public | BindingFlags.NonPublic)!
            .SetValue(session, int.MaxValue);

        var error = await Assert.ThrowsAsync<InvalidStateException>(() =>
            SessionKeyEndpoints.ClaimNextGenerationAsync(
                session.Id.Value,
                CreateClaimer(session),
                session.OwnerUserId,
                session.IncarnationId,
                session.CurrentKeyGeneration,
                TestContext.Current.CancellationToken));

        Assert.Equal("Session key generation is exhausted.", error.Message);
    }

    [Fact]
    public async Task Claim_And_Current_Generation_Reject_Stale_Incarnation()
    {
        var session = CreateSession();
        var staleIncarnationId = Guid.CreateVersion7();

        var claimError = await Assert.ThrowsAsync<InvalidStateException>(() =>
            SessionKeyEndpoints.ClaimNextGenerationAsync(
                session.Id.Value,
                CreateClaimer(session),
                session.OwnerUserId,
                staleIncarnationId,
                session.CurrentKeyGeneration,
                TestContext.Current.CancellationToken));
        var queryError = await Assert.ThrowsAsync<InvalidStateException>(() =>
            SessionKeyEndpoints.ReadCurrentGenerationAsync(
                session.Id.Value,
                CreateClaimer(session),
                session.OwnerUserId,
                staleIncarnationId,
                TestContext.Current.CancellationToken));

        Assert.Contains("incarnation changed", claimError.Message, StringComparison.Ordinal);
        Assert.Contains("incarnation changed", queryError.Message, StringComparison.Ordinal);
        Assert.Equal(0, session.CurrentKeyGeneration);
    }

    private static SessionKeyGenerationClaimer CreateClaimer(Session session) =>
        new(
            new EndpointSessionRepository(session),
            new EndpointUnitOfWork());

    private static Session CreateSession() =>
        Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            UserId.New(),
            "session",
            SessionScope.Friends,
            ToolKind.Terminal,
            AccessLevel.View,
            "secret");

    private sealed class EndpointSessionRepository(Session session)
        : SessionRepositoryStub
    {
        public override Task<Session?> GetByIdForUpdateAsync(
            SessionId id,
            CancellationToken ct = default) =>
            Task.FromResult<Session?>(session.Id == id ? session : null);

        public override Task UpdateAsync(
            Session updated,
            CancellationToken ct = default) =>
            Task.CompletedTask;
    }

    private sealed class EndpointUnitOfWork : UnitOfWorkStub
    {
        public override Task SaveChangesAsync(CancellationToken ct = default) =>
            Task.CompletedTask;

        public override Task<ITransactionScope> BeginTransactionAsync(
            CancellationToken ct = default) =>
            Task.FromResult<ITransactionScope>(new CompletedTransactionScope());
    }
}
