pragma Singleton

import QtQuick

QtObject {
    property var spots: []
    property var flying: ({})

    function enroll(spot) {
        spots.push(spot)
    }
    function leave(spot) {
        const index = spots.indexOf(spot)
        if (index >= 0)
            spots.splice(index, 1)
    }
    function hides(key, place) {
        return flying[key + "@" + place] === true
    }
    function launch(keys) {
        const next = Object.assign({}, flying)
        for (const key of keys)
            next[key] = true
        flying = next
    }
    function land(key) {
        if (flying[key] !== true)
            return
        const next = Object.assign({}, flying)
        delete next[key]
        flying = next
    }
}
