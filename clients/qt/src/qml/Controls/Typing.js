.pragma library

function generator(seed) {
    let state = seed
    return function () {
        state = (state + 0x6d2b79f5) | 0
        let value = Math.imul(state ^ (state >>> 15), 1 | state)
        value = (value + Math.imul(value ^ (value >>> 7), 61 | value)) ^ value
        return ((value ^ (value >>> 14)) >>> 0) / 4294967296
    }
}

const offsets = (function () {
    const random = generator(0x2545f491)
    const result = []
    let burst = 0
    let sum = 0
    while (result.length < 64) {
        if (burst === 0) {
            burst = 5 + Math.floor(random() * 9)
            sum += 0.3 + random() * 0.45
        } else {
            sum += 0.045 + random() * 0.07
        }
        burst -= 1
        result.push(sum)
    }
    return result
})()
const period = offsets[offsets.length - 1]

function wrapped(value, count) {
    return ((value % count) + count) % count
}

function step(seconds) {
    const round = Math.floor(seconds / period)
    const within = seconds - round * period
    let index = offsets.findIndex(offset => offset > within)
    if (index < 0)
        index = offsets.length
    return round * offsets.length + index - 1
}

function time(n) {
    return Math.floor(n / offsets.length) * period + offsets[wrapped(n, offsets.length)]
}
