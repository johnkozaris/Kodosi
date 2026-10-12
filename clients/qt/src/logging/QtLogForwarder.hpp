#pragma once

namespace kodosi {

/// Copies Qt messages into the app's log for as long as it lives. Qt's
/// previous handler still prints them.
class QtLogForwarder final {
public:
    QtLogForwarder();
    ~QtLogForwarder();

    QtLogForwarder(const QtLogForwarder&) = delete;
    QtLogForwarder& operator=(const QtLogForwarder&) = delete;
};

}
