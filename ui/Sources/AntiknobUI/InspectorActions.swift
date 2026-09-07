// InspectorActions.swift — the process and device work behind InspectorPane.
//
// Split out for the same reason as LayerBindings.swift: the view struct
// crossed the body-length cap once its rows became grids, and there was
// already a seam at "// MARK: - Snoop Execution". Views above, work below;
// nothing in this file draws anything.

import AppKit
import Foundation
import SwiftUI

extension InspectorPane {

    func startSnooping() {
        isSnooping = true
        let task = Process()
        let pipe = Pipe()

        var possiblePaths: [String] = [
            "/Applications/Antiknob/bin/antiknob",
            "\(FileManager.default.homeDirectoryForCurrentUser.path)/.local/bin/antiknob",
            "/usr/local/bin/antiknob"
        ]
        if let res = Bundle.main.resourcePath {
            possiblePaths.append("\(res)/antiknob")
        }

        guard let exec = possiblePaths.first(where: { FileManager.default.isExecutableFile(atPath: $0) }) else {
            isSnooping = false
            return
        }

        task.executableURL = URL(fileURLWithPath: exec)
        task.arguments = ["listen", "--timeout-secs", "30"]
        task.standardOutput = pipe
        task.standardError = Pipe()

        pipe.fileHandleForReading.readabilityHandler = { handle in
            let data = handle.availableData
            guard !data.isEmpty, let text = String(data: data, encoding: .utf8) else { return }
            let lines = text.components(separatedBy: .newlines).filter { !$0.isEmpty }
            DispatchQueue.main.async {
                for line in lines {
                    if line.contains("[") && line.contains("B:") {
                        parseSnoopLine(line)
                    }
                }
            }
        }

        task.terminationHandler = { _ in
            DispatchQueue.main.async {
                self.isSnooping = false
            }
        }

        do {
            try task.run()
            self.snoopProcess = task
        } catch {
            isSnooping = false
        }
    }

    func stopSnooping() {
        snoopProcess?.terminate()
        snoopProcess = nil
        isSnooping = false
    }

    func parseSnoopLine(_ line: String) {
        let parts = line.components(separatedBy: "   ")
        let head = parts.first ?? line
        let decode = parts.count > 1 ? parts[1] : ""
        let item = PacketLogItem(
            timestamp: Date(),
            iface: "HID",
            bytesHex: head.trimmingCharacters(in: .whitespaces),
            decode: decode.trimmingCharacters(in: .whitespaces)
        )
        logs.insert(item, at: 0)
        if logs.count > 100 { logs.removeLast() }
    }

    func readSlotTable() {
        isReadingSlots = true
        slotError = nil
        slotRecords = []

        store.readSlots(group: slotGroup, counters: [1, 2, 3]) { result in
            isReadingSlots = false
            switch result {
            case .success(let slots):
                var recs: [SlotDumpRecord] = []
                for s in slots {
                    let g = (s["group"] as? UInt8) ?? slotGroup
                    let c = (s["counter"] as? UInt8) ?? 0
                    let h = (s["hex"] as? String) ?? (s["error"] as? String ?? "")
                    recs.append(SlotDumpRecord(group: g, counter: c, hex: h))
                }
                slotRecords = recs
            case .failure(let err):
                slotError = err.localizedDescription
            }
        }
    }

    func sendRaw() {
        rawPacketStatus = "Sending..."
        store.sendRawPacket(hexString: rawPacketInput) { result in
            switch result {
            case .success(let msg):
                rawPacketStatus = "Success: \(msg)"
            case .failure(let err):
                rawPacketStatus = "Error: \(err.localizedDescription)"
            }
        }
    }

    func timeStr(_ date: Date) -> String {
        let f = DateFormatter()
        f.dateFormat = "HH:mm:ss.SSS"
        return f.string(from: date)
    }
}
