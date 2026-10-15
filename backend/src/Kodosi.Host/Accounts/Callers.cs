using Kodosi.Admission;
using Kodosi.Data;
using Kodosi.Devices;

namespace Kodosi.Accounts;

/// <summary>The account of a signed-in request.</summary>
public sealed record SignedIn(User User)
{
    public static async ValueTask<SignedIn?> BindAsync(HttpContext context) =>
        new(await context.RequestServices.GetRequiredService<CurrentUser>().GetAsync(context, context.RequestAborted));
}

/// <summary>
/// The account and the approved device of a request that proves a device session. A request that changes data checks
/// its device in the turn of its account, so that a device that an earlier write removed cannot write.
/// </summary>
public sealed record DeviceCaller(User User, ApprovedDevice Device)
{
    public static async ValueTask<DeviceCaller?> BindAsync(HttpContext context)
    {
        var (user, device) = await ProveAsync(context, allowExpiredList: false);
        return new DeviceCaller(user, device);
    }

    internal static async Task<(User User, ApprovedDevice Device)> ProveAsync(HttpContext context, bool allowExpiredList)
    {
        var services = context.RequestServices;
        var user = await services.GetRequiredService<CurrentUser>().GetAsync(context, context.RequestAborted);
        await services.GetRequiredService<AccountGate>().AdmitAsync(context);
        return (user, await services.GetRequiredService<DeviceService>().RequireProofAsync(context, user.Id, context.RequestAborted, allowExpiredList));
    }
}

/// <summary>A device that renews the signed device list of its account. The list may have expired.</summary>
public sealed record RenewingDevice(User User, ApprovedDevice Device)
{
    public static async ValueTask<RenewingDevice?> BindAsync(HttpContext context)
    {
        var (user, device) = await DeviceCaller.ProveAsync(context, allowExpiredList: true);
        return new RenewingDevice(user, device);
    }
}
