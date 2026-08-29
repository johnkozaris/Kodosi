using System.ComponentModel.DataAnnotations;

namespace Kodosi.Application;

public sealed record FriendRequestHandleRequest(
    [property: Required, StringLength(64, MinimumLength = 1)]
    string Username);
