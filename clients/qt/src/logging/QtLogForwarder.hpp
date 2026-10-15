#pragma once

namespace kodosi {

class QtLogForwarder final {
public:
    QtLogForwarder();
    ~QtLogForwarder();

    QtLogForwarder(const QtLogForwarder&) = delete;
    QtLogForwarder& operator=(const QtLogForwarder&) = delete;
};

}
