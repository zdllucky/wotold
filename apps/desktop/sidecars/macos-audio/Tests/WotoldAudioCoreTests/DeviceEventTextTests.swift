import Testing

@testable import WotoldAudioCore

@Suite("DeviceEventText")
struct DeviceEventTextTests {
    @Test("Дорожки называются по-разному и причина у каждой своя")
    func legsAreDistinguishable() {
        let mic = DeviceEventText.lost(leg: .mic, silentSec: 3.2)
        let system = DeviceEventText.lost(leg: .system, silentSec: 3.2)

        #expect(mic.contains("микрофонная"))
        #expect(system.contains("системная"))
        #expect(mic != system, "по сообщению должно быть видно, какая дорожка упала")
        #expect(mic.contains("устройство ввода"))
        #expect(system.contains("устройство вывода"))
    }

    @Test("Время молчания — одна цифра после запятой")
    func silenceIsFormattedOnce() {
        #expect(DeviceEventText.lost(leg: .mic, silentSec: 3.24).contains("3.2 с"))
        #expect(DeviceEventText.lost(leg: .system, silentSec: 12.0).contains("12.0 с"))
    }

    @Test("Отрицательное и нечисловое время не утекает в UI")
    func brokenDurationsClampToZero() {
        // Часы монотонные, но время считается вычитанием: перепутанный
        // порядок аргументов у вызывающего дал бы «-4.0 с» в баннере.
        #expect(DeviceEventText.lost(leg: .system, silentSec: -4).contains("0.0 с"))
        #expect(DeviceEventText.lost(leg: .mic, silentSec: .nan).contains("0.0 с"))
        #expect(DeviceEventText.lost(leg: .mic, silentSec: .infinity).contains("0.0 с"))
    }
}
