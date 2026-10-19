using System.Security.Cryptography;
using System.Text.Json;
using Kodosi.Data;
using Kodosi.Devices;
using Kodosi.Security;
using Microsoft.EntityFrameworkCore;

namespace Kodosi.Missions;

public sealed partial class MissionService
{
    private const int MaximumKeyStateBytes = 3 * 1024 * 1024;
    private const int MaximumItemBytes = 64 * 1024;
    private const int KeyHistoryPageBytes = 4 * 1024 * 1024;
    private const int KeyHistoryPageStates = 256;

    public const long DosiRoomBytes = 4 * 1024 * 1024;
    public const int RoomPublicKeyLength = 1184 + 32;

    internal const int MaxRoomRecipients = (Limits.MaxDevicesPerUser + 1) * (Limits.MaxMissionMembers + 1);

    internal static (byte[] Key, byte[] Signature) ValidRoomKey(Guid userId, string deviceId, byte[] signingKey, RecipientKeyWrite request)
    {
        var key = Limits.Base64(request.PublicKey, "Room public key", RoomPublicKeyLength);
        var signature = Limits.Base64(request.Signature, "Room public key signature", IdentityWireFormat.MlDsa65SignatureLength);
        if (key.Length != RoomPublicKeyLength || !SignatureVerifier.Verify(signingKey, Proofs.RoomRecipient(userId, deviceId, key), signature))
            throw ApiException.Invalid("Invalid room public key.");
        return (key, signature);
    }

    public async Task RegisterRoomKeyAsync(Guid userId, ApprovedDevice device, RecipientKeyWrite request, CancellationToken ct)
    {
        var (key, signature) = ValidRoomKey(userId, device.Id, device.SigningPublicKey, request);
        var existing = await db.RoomRecipientKeys.SingleOrDefaultAsync(x => x.DeviceId == device.Id, ct);
        if (existing is not null)
        {
            if (!existing.PublicKey.AsSpan().SequenceEqual(key)) throw ApiException.Conflict("This device already has a different room key.");
            return;
        }
        db.RoomRecipientKeys.Add(new RoomRecipientKey { DeviceId = device.Id, UserId = userId, PublicKey = key, Signature = signature });
        await db.SaveChangesAsync(ct);
        await RoomMembership.NotifyRoomsOfAsync(db, connections, userId, ct);
    }

    public async Task<object> RecipientKeysAsync(Guid caller, Guid userId, CancellationToken ct)
    {
        await devices.RequireRelationAsync(caller, userId, ct);
        var keys = await (from key in db.RoomRecipientKeys.AsNoTracking()
                          join device in db.Devices on key.DeviceId equals device.Id
                          where key.UserId == userId && !device.Revoked
                          select key).ToListAsync(ct);
        return keys.Select(key => new { key.UserId, key.DeviceId, publicKey = Convert.ToBase64String(key.PublicKey), signature = Convert.ToBase64String(key.Signature) });
    }

    public async Task<object> KeyHistoryAsync(Guid id, Guid userId, long afterVersion, CancellationToken ct)
    {
        var room = await db.Missions.AsNoTracking().SingleOrDefaultAsync(x => x.Id == id, ct) ?? throw ApiException.Missing();
        var members = await MemberIdsAsync(id, ct, invited: true);
        if (!members.Contains(userId)) throw ApiException.Missing();
        if (afterVersion < 0) throw ApiException.Invalid("Invalid room key position.");
        var sizes = await db.MissionKeyStates.Where(x => x.MissionId == id && x.Version > afterVersion).OrderBy(x => x.Version)
            .Select(x => new { x.Version, Size = (long)x.Body.Length + x.Signature.Length }).Take(KeyHistoryPageStates).ToListAsync(ct);
        long bytes = 0, last = afterVersion;
        foreach (var state in sizes)
        {
            if (last != afterVersion && bytes + state.Size > KeyHistoryPageBytes) break;
            bytes += state.Size;
            last = state.Version;
        }
        var states = last == afterVersion ? [] : await db.MissionKeyStates.AsNoTracking()
            .Where(x => x.MissionId == id && x.Version > afterVersion && x.Version <= last).OrderBy(x => x.Version).ToListAsync(ct);
        return new { ownerUserId = room.OwnerUserId, version = room.KeyVersion, members, states = states.Select(StateDto) };
    }

    private static object StateDto(MissionKeyState state) => new
    { state.Version, body = Convert.ToBase64String(state.Body), signature = Convert.ToBase64String(state.Signature) };

    public async Task StoreKeysAsync(Guid id, Guid userId, RoomKeyWrite request, CancellationToken ct)
    {
        var keys = await PrepareKeysAsync(userId, request, ct);
        await using var transaction = await db.Database.BeginTransactionAsync(ct);
        var room = await RequireMemberForUpdateAsync(id, userId, ct);
        await AppendKeysAsync(room, keys, ct);
        await transaction.CommitAsync(ct);
        connections.Notify(await MemberIdsAsync(id, ct), "rooms", id);
    }

    private async Task<PreparedKeys?> PrepareKeysAsync(Guid userId, RoomKeyWrite? request, CancellationToken ct)
    {
        if (request is null) return null;
        var device = await devices.RequireDeviceAsync(userId, request.DeviceId, ct);
        var body = Limits.Base64(request.Body, "Room keys", MaximumKeyStateBytes);
        var signature = Limits.Base64(request.Signature, "Room signature", IdentityWireFormat.MlDsa65SignatureLength);
        if (!SignatureVerifier.Verify(device.SigningPublicKey, Proofs.Tagged(DomainTags.RoomStateV1, body), signature))
            throw ApiException.Forbidden("Invalid room signature.");
        var state = ReadBody<KeyStateBody>(body);
        if (state.AuthorId != userId || state.DeviceId != device.Id || state.Epoch < 1)
            throw ApiException.Conflict("Room keys changed; refresh before retrying.");
        if (state.Recipients.Length == 0 || state.Recipients.Length > MaxRoomRecipients)
            throw ApiException.Invalid("Invalid room recipients.");
        return new PreparedKeys(device, body, signature, state);
    }

    private async Task AppendKeysAsync(Mission room, PreparedKeys? keys, CancellationToken ct)
    {
        if (keys is null)
        {
            if (room.KeyVersion != 0) throw ApiException.Conflict("Refresh the room before changing its members.");
            return;
        }
        var state = keys.State;
        var (version, epoch, userId) = (state.Version, state.Epoch, state.AuthorId);
        if (state.RoomId != room.Id || state.OwnerUserId != room.OwnerUserId || version != room.KeyVersion + 1)
            throw ApiException.Conflict("Room keys changed; refresh before retrying.");
        var members = state.Members.Keys.ToHashSet();
        if (!members.SetEquals(await MemberIdsAsync(room.Id, ct, invited: true)))
            throw ApiException.Conflict("Room membership changed; refresh before retrying.");
        var previous = room.KeyVersion == 0 ? null : await db.MissionKeyStates.AsNoTracking().SingleAsync(x => x.MissionId == room.Id && x.Version == room.KeyVersion, ct);
        if (previous is null)
        {
            if (userId != room.OwnerUserId || epoch != 1 || state.PreviousHash != "")
                throw ApiException.Invalid("The room owner must create its first keys.");
        }
        else
        {
            if (state.PreviousHash != Convert.ToHexStringLower(SHA256.HashData(previous.Body))
                || epoch < previous.Epoch || epoch > previous.Epoch + 1) throw ApiException.Conflict("Room key history changed.");
            var oldMembers = ReadBody<KeyStateBody>(previous.Body).Members.Keys.ToHashSet();
            var added = members.Except(oldMembers).ToArray();
            var removed = oldMembers.Except(members).ToArray();
            if (userId != room.OwnerUserId && (added.Length != 0 || removed.Any(x => x != userId))) throw ApiException.Forbidden();
            if (removed.Length != 0 && epoch != previous.Epoch + 1) throw ApiException.Invalid("Leaving members requires new room keys.");
        }
        if (state.Recipients.Any(x => !members.Contains(x.UserId))) throw ApiException.Invalid("Invalid room recipients.");
        db.MissionKeyStates.Add(new MissionKeyState { MissionId = room.Id, Version = version, Epoch = epoch, Body = keys.Body, Signature = keys.Signature });
        room.KeyVersion = version;
        await db.SaveChangesAsync(ct);
    }

    public async Task<object> ContentAsync(Guid id, Guid userId, string? kind, long after, long? before, int? limit, CancellationToken ct)
    {
        var room = await RequireMemberAsync(id, userId, ct);
        if (after < 0 || before is <= 0 || limit is < 1 or > 200 || kind is not (null or "message" or "task" or "repository"))
            throw ApiException.Invalid("Invalid room history request.");
        var query = db.MissionItems.AsNoTracking().Where(x => x.MissionId == id && (kind == null || x.Kind == kind)
            && x.Sequence > after && (before == null || x.Sequence < before));
        var take = limit ?? 100;
        var items = before != null ? await query.OrderByDescending(x => x.Sequence).Take(take + 1).ToListAsync(ct)
            : await query.OrderBy(x => x.Sequence).Take(take + 1).ToListAsync(ct);
        var selected = new List<MissionItem>();
        var bytes = 0;
        foreach (var item in items.Take(take))
        {
            if (bytes + item.Body.Length + item.Signature.Length > 2 * 1024 * 1024) break;
            bytes += item.Body.Length + item.Signature.Length; selected.Add(item);
        }
        var more = items.Count > selected.Count;
        return new { sequence = room.ContentSequence, keyVersion = room.KeyVersion, hasMore = more,
            historyTrimmed = room.HistoryTrimmed, items = selected.OrderBy(x => x.Sequence).Select(ItemDto) };
    }

    private static object ItemDto(MissionItem item) => new
    { item.Id, item.Kind, item.Version, item.Sequence, item.KeyVersion, item.UserId, item.DeviceId, item.CreatedAt,
        body = Convert.ToBase64String(item.Body), signature = Convert.ToBase64String(item.Signature) };

    public async Task<object> PutContentAsync(Guid id, Guid itemId, Guid userId, ContentWrite request, CancellationToken ct, ApprovedDevice? signer = null)
    {
        var device = signer is not null && signer.UserId == userId && signer.Id == request.DeviceId
            ? signer
            : await devices.RequireDeviceAsync(userId, request.DeviceId, ct);
        var body = Limits.Base64(request.Body, "Room content", MaximumItemBytes);
        var signature = Limits.Base64(request.Signature, "Content signature", IdentityWireFormat.MlDsa65SignatureLength);
        if (!SignatureVerifier.Verify(device.SigningPublicKey, Proofs.Tagged(DomainTags.RoomContentV1, body), signature))
            throw ApiException.Forbidden("Invalid room content signature.");
        var content = ReadBody<ContentBody>(body);
        var kind = content.Kind;
        var keyVersion = content.KeyVersion;
        var version = content.Version;
        if (kind is not ("message" or "task" or "repository") || content.RoomId != id
            || content.Id != itemId || content.AuthorId != userId
            || content.DeviceId != device.Id || request.ExpectedVersion < 0 || version != request.ExpectedVersion + 1)
            throw ApiException.Invalid("Invalid room content identity.");
        if (Limits.Base64(content.Nonce, "Content nonce", 12).Length != 12
            || Limits.Base64(content.Ciphertext, "Encrypted content", MaximumItemBytes).Length < 16)
            throw ApiException.Invalid("Invalid encrypted room content.");
        await using var transaction = await db.Database.BeginTransactionAsync(ct);
        var room = await RequireMemberForUpdateAsync(id, userId, ct);
        var existing = await db.MissionItems.SingleOrDefaultAsync(x => x.MissionId == id && x.Id == itemId, ct);
        if (existing is not null && existing.Body.AsSpan().SequenceEqual(body) && existing.Signature.AsSpan().SequenceEqual(signature)) return ItemDto(existing);
        if (room.KeyVersion == 0 || keyVersion != room.KeyVersion) throw ApiException.Conflict("Room keys changed; refresh before posting.");
        var epoch = await db.MissionKeyStates.Where(x => x.MissionId == id && x.Version == keyVersion).Select(x => x.Epoch).SingleAsync(ct);
        if (content.Epoch != epoch) throw ApiException.Invalid("The content belongs to another room key epoch.");
        if ((existing?.Version ?? 0) != request.ExpectedVersion || (existing is not null && (existing.Kind != kind || kind == "message")))
            throw ApiException.Conflict("This room item changed; refresh before retrying.");
        if (existing is null && kind != "message" && await db.MissionItems.CountAsync(x => x.MissionId == id && x.Kind == kind, ct) >= 4096)
            throw ApiException.Conflict("The room has reached its item limit.");
        room.ContentBytes += (long)body.Length + signature.Length - (existing is null ? 0 : (long)existing.Body.Length + existing.Signature.Length);
        var planned = (await plans.LimitsAsync(db, room.OwnerUserId, ct)).RoomBytes;
        if ((room.Dosi ? Math.Min(planned ?? DosiRoomBytes, DosiRoomBytes) : planned) is { } most && room.ContentBytes > most)
            await TrimHistoryAsync(room, most, ct);
        var item = existing ?? new MissionItem { MissionId = id, Id = itemId, CreatedAt = clock.GetUtcNow() };
        item.Kind = kind; item.Version = version; item.KeyVersion = keyVersion;
        room.ContentSequence = checked(room.ContentSequence + 1);
        item.Sequence = room.ContentSequence; item.Body = body; item.Signature = signature;
        item.UserId = userId; item.DeviceId = device.Id;
        if (existing is null) db.MissionItems.Add(item);
        await db.SaveChangesAsync(ct); await transaction.CommitAsync(ct);
        connections.Notify(await MemberIdsAsync(id, ct), "rooms", id, item.Sequence);
        return ItemDto(item);
    }

    private async Task TrimHistoryAsync(Mission room, long most, CancellationToken ct)
    {
        var excess = room.ContentBytes - most;
        var removed = new List<Guid>();
        long freed = 0, after = 0;
        while (freed < excess)
        {
            var oldest = await db.MissionItems.AsNoTracking()
                .Where(x => x.MissionId == room.Id && x.Kind == "message" && x.Sequence > after).OrderBy(x => x.Sequence).Take(64)
                .Select(x => new { x.Id, x.Sequence, Size = (long)x.Body.Length + x.Signature.Length }).ToListAsync(ct);
            if (oldest.Count == 0)
                throw ApiException.Forbidden("This room is full: its tasks and repositories use all of its space. Make a new room to continue.");
            foreach (var message in oldest.TakeWhile(_ => freed < excess))
            {
                removed.Add(message.Id); freed += message.Size; after = message.Sequence;
            }
        }
        await db.MissionItems.Where(x => x.MissionId == room.Id && removed.Contains(x.Id)).ExecuteDeleteAsync(ct);
        room.ContentBytes -= freed;
        room.HistoryTrimmed = true;
    }

    private static T ReadBody<T>(byte[] body) where T : class => JsonSerializer.Deserialize<T>(body, StrictJson.Options) ?? throw ApiException.Invalid("Invalid room data.");
    private sealed record PreparedKeys(ApprovedDevice Device, byte[] Body, byte[] Signature, KeyStateBody State);
    private sealed record SealedBody(string Nonce, string Ciphertext);
    private sealed record RecipientBody(Guid UserId, string DeviceId, string KemCiphertext, SealedBody Sealed);
    private sealed record KeyStateBody(Guid RoomId, Guid OwnerUserId, Guid AuthorId, string DeviceId,
        long Version, long Epoch, long CreatedAtMs, string PreviousHash, Dictionary<Guid, JsonElement> Members,
        RecipientBody[] Recipients, SealedBody? PreviousKey);
    private sealed record ContentBody(Guid RoomId, Guid Id, string Kind, long Version, long KeyVersion,
        long Epoch, Guid AuthorId, string DeviceId, string Nonce, string Ciphertext);

    public sealed record RecipientKeyWrite(string PublicKey, string Signature);
    public sealed record RoomKeyWrite(string DeviceId, string Body, string Signature);
    public sealed record ContentWrite(long ExpectedVersion, string DeviceId, string Body, string Signature);
    public sealed record MembershipWrite(RoomKeyWrite? Keys = null);
}
