using Kodosi.Domain;

namespace Kodosi.Application;

public interface ISignedDeviceListVerifier
{
    SignedDeviceListParser.ParsedSignedDeviceList Parse(ReadOnlySpan<byte> body);

    bool Verify(
        ReadOnlySpan<byte> body,
        ReadOnlySpan<byte> signature,
        ReadOnlySpan<byte> signerPublicKey);
}

public interface IDeviceListRealtimeEffects
{
    Task PersistAndEnforceAsync(
        UserId userId,
        IReadOnlyCollection<string> revokedDeviceIds,
        IReadOnlyCollection<DeviceRevocationSessionTarget> sessionsWithRevokedKeys,
        Func<CancellationToken, Task> persistAndCommitAsync,
        CancellationToken ct = default);

    Task EnforceCommittedAsync(
        UserId userId,
        IReadOnlyCollection<string> revokedDeviceIds,
        IReadOnlyCollection<DeviceRevocationSessionTarget> sessionsWithRevokedKeys,
        CancellationToken ct = default);

    void ReportEnforcementPending(Guid revocationId, Exception exception);

    void PublishChanged(
        IReadOnlyCollection<UserId> audience,
        UserId userId,
        long generation);
}
