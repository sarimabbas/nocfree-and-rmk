import CoreBluetooth
import Foundation

// Read RMK's two standard Battery Service instances on an already-connected keyboard.
final class BatteryReader: NSObject, CBCentralManagerDelegate, CBPeripheralDelegate {
    private let batteryService = CBUUID(string: "180F")
    private let batteryLevel = CBUUID(string: "2A19")
    private let userDescription = CBUUID(string: "2901")
    private var central: CBCentralManager!
    private var keyboard: CBPeripheral?
    private var values: [ObjectIdentifier: UInt8] = [:]
    private var names: [ObjectIdentifier: String] = [:]
    private var expected = 0
    private(set) var finished = false

    override init() {
        super.init()
        central = CBCentralManager(delegate: self, queue: nil)
    }

    func centralManagerDidUpdateState(_ central: CBCentralManager) {
        guard central.state == .poweredOn else { finish(); return }
        let matches = central.retrieveConnectedPeripherals(withServices: [batteryService])
            .filter { $0.name == "NocFree RMK" }
        guard matches.count == 1, let keyboard = matches.first else { finish(); return }
        self.keyboard = keyboard
        keyboard.delegate = self
        central.connect(keyboard)
    }

    func centralManager(_ central: CBCentralManager, didConnect peripheral: CBPeripheral) {
        peripheral.discoverServices([batteryService])
    }

    func centralManager(_ central: CBCentralManager, didFailToConnect peripheral: CBPeripheral, error: Error?) {
        finish()
    }

    func peripheral(_ peripheral: CBPeripheral, didDiscoverServices error: Error?) {
        guard error == nil else { finish(); return }
        let services = (peripheral.services ?? []).filter { $0.uuid == batteryService }
        guard !services.isEmpty else { finish(); return }
        for service in services { peripheral.discoverCharacteristics([batteryLevel], for: service) }
    }

    func peripheral(_ peripheral: CBPeripheral, didDiscoverCharacteristicsFor service: CBService, error: Error?) {
        guard error == nil else { return }
        for characteristic in service.characteristics ?? [] where characteristic.uuid == batteryLevel {
            expected += 1
            peripheral.discoverDescriptors(for: characteristic)
            peripheral.readValue(for: characteristic)
        }
    }

    func peripheral(_ peripheral: CBPeripheral, didDiscoverDescriptorsFor characteristic: CBCharacteristic, error: Error?) {
        guard error == nil else { return }
        for descriptor in characteristic.descriptors ?? [] where descriptor.uuid == userDescription {
            peripheral.readValue(for: descriptor)
        }
    }

    func peripheral(_ peripheral: CBPeripheral, didUpdateValueFor descriptor: CBDescriptor, error: Error?) {
        guard error == nil, descriptor.uuid == userDescription,
              let characteristic = descriptor.characteristic else { return }
        if let name = descriptor.value as? String {
            names[ObjectIdentifier(characteristic)] = name
        } else if let data = descriptor.value as? Data,
                  let name = String(data: data, encoding: .utf8) {
            names[ObjectIdentifier(characteristic)] = name
        }
        completeIfReady()
    }

    func peripheral(_ peripheral: CBPeripheral, didUpdateValueFor characteristic: CBCharacteristic, error: Error?) {
        guard error == nil, let byte = characteristic.value?.first, byte <= 100 else { return }
        values[ObjectIdentifier(characteristic)] = byte
        completeIfReady()
    }

    private func completeIfReady() {
        if expected >= 2 && values.count >= expected && names.count >= expected { finish() }
    }

    func finish() {
        guard !finished else { return }
        finished = true
        var levels: [String: UInt8] = [:]
        for (key, name) in names where name == "Left" || name == "Right" {
            if let value = values[key] { levels[name] = value }
        }
        if let data = try? JSONSerialization.data(withJSONObject: levels, options: [.sortedKeys]),
           let text = String(data: data, encoding: .utf8) { print(text) }
        if let keyboard { central.cancelPeripheralConnection(keyboard) }
    }
}

let reader = BatteryReader()
let deadline = Date().addingTimeInterval(4)
while !reader.finished && Date() < deadline {
    CFRunLoopRunInMode(.defaultMode, 0.1, true)
}
reader.finish()
