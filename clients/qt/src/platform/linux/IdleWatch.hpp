#pragma once

#include <chrono>
#include <functional>
#include <memory>

namespace kodosi {

class IdleWatch {
public:
    using Quiet = std::function<void(bool quiet)>;

    virtual ~IdleWatch() = default;
    virtual void watch(std::chrono::milliseconds quietTime) = 0;
};

[[nodiscard]] std::unique_ptr<IdleWatch> waylandIdleWatch(IdleWatch::Quiet quiet);
[[nodiscard]] std::unique_ptr<IdleWatch> mutterIdleWatch(IdleWatch::Quiet quiet);
[[nodiscard]] std::unique_ptr<IdleWatch> x11IdleWatch(IdleWatch::Quiet quiet);

}
