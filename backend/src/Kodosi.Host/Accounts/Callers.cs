using Kodosi.Admission;
using Kodosi.Data;
using Kodosi.Devices;

namespace Kodosi.Accounts;

public sealed record SignedIn(User User)
{
    public static async ValueTask<SignedIn?> BindAsync(HttpContext context) =>
        new(await context.RequestServices.GetRequiredService<CurrentUser>().GetAsync(context, context.RequestAborted));
}

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

public sealed record RenewingDevice(User User, ApprovedDevice Device)
{
    public static async ValueTask<RenewingDevice?> BindAsync(HttpContext context)
    {
        var (user, device) = await DeviceCaller.ProveAsync(context, allowExpiredList: true);
        return new RenewingDevice(user, device);
    }
}
