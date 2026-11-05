import AVFoundation
import CoreMedia
import ScreenCaptureKit

public typealias AudioCallback = @convention(c) (UnsafePointer<Float>?, Int32, UnsafeMutableRawPointer?) -> Void

private final class OpenSystemAudio: NSObject, SCStreamOutput, SCStreamDelegate {
    private let callback: AudioCallback
    private let context: UnsafeMutableRawPointer?
    private let queue = DispatchQueue(label: "com.openflow.system-audio", qos: .userInitiated)
    private var stream: SCStream?

    init(callback: @escaping AudioCallback, context: UnsafeMutableRawPointer?) {
        self.callback = callback
        self.context = context
    }

    func start() throws {
        let contentGate = DispatchSemaphore(value: 0)
        var contentResult: Result<SCShareableContent, Error>?
        SCShareableContent.getExcludingDesktopWindows(false, onScreenWindowsOnly: false) {
            content, error in
            if let error { contentResult = .failure(error) }
            else if let content, !content.displays.isEmpty { contentResult = .success(content) }
            else { contentResult = .failure(NSError(domain: "OpenFlowAudio", code: 1, userInfo: [NSLocalizedDescriptionKey: "No display is available for Mac audio capture."])) }
            contentGate.signal()
        }
        guard contentGate.wait(timeout: .now() + 20) == .success, let result = contentResult else {
            throw NSError(domain: "OpenFlowAudio", code: 2, userInfo: [NSLocalizedDescriptionKey: "Timed out while requesting macOS audio-capture permission."])
        }
        let content = try result.get()
        guard let display = content.displays.first else {
            throw NSError(domain: "OpenFlowAudio", code: 3, userInfo: [NSLocalizedDescriptionKey: "No display is available for Mac audio capture."])
        }

        let filter = SCContentFilter(display: display, excludingApplications: [], exceptingWindows: [])
        let config = SCStreamConfiguration()
        config.width = 2
        config.height = 2
        config.queueDepth = 1
        config.capturesAudio = true
        if #available(macOS 15.0, *) { config.captureMicrophone = false }
        config.excludesCurrentProcessAudio = true
        config.sampleRate = 16_000
        config.channelCount = 1

        let stream = SCStream(filter: filter, configuration: config, delegate: self)
        try stream.addStreamOutput(self, type: .audio, sampleHandlerQueue: queue)
        let startGate = DispatchSemaphore(value: 0)
        var startError: Error?
        stream.startCapture { error in startError = error; startGate.signal() }
        guard startGate.wait(timeout: .now() + 20) == .success else {
            throw NSError(domain: "OpenFlowAudio", code: 4, userInfo: [NSLocalizedDescriptionKey: "Timed out while starting Mac audio capture."])
        }
        if let startError { throw startError }
        self.stream = stream
    }

    func stop() {
        guard let stream else { return }
        let gate = DispatchSemaphore(value: 0)
        stream.stopCapture { _ in gate.signal() }
        gate.wait()
        self.stream = nil
    }

    func stream(_ stream: SCStream, didStopWithError error: Error) {
        NSLog("[OpenFlow] Mac audio capture stopped: %@", error.localizedDescription)
    }

    func stream(_ stream: SCStream, didOutputSampleBuffer sampleBuffer: CMSampleBuffer, of type: SCStreamOutputType) {
        guard type == .audio, CMSampleBufferDataIsReady(sampleBuffer),
              let format = CMSampleBufferGetFormatDescription(sampleBuffer),
              let asbd = CMAudioFormatDescriptionGetStreamBasicDescription(format)?.pointee,
              asbd.mFormatID == kAudioFormatLinearPCM,
              let block = CMSampleBufferGetDataBuffer(sampleBuffer) else { return }

        var length = 0
        var dataPointer: UnsafeMutablePointer<Int8>?
        guard CMBlockBufferGetDataPointer(block, atOffset: 0, lengthAtOffsetOut: nil, totalLengthOut: &length, dataPointerOut: &dataPointer) == kCMBlockBufferNoErr,
              let dataPointer, length > 0 else { return }
        let frames = Int(CMSampleBufferGetNumSamples(sampleBuffer))
        let channels = max(1, Int(asbd.mChannelsPerFrame))
        let bytesPerSample = Int(asbd.mBitsPerChannel / 8)
        guard frames > 0, bytesPerSample > 0 else { return }
        let flags = asbd.mFormatFlags
        let isFloat = (flags & kAudioFormatFlagIsFloat) != 0
        let isBigEndian = (flags & kAudioFormatFlagIsBigEndian) != 0
        let isNonInterleaved = (flags & kAudioFormatFlagIsNonInterleaved) != 0
        let bytesPerFrame = Int(asbd.mBytesPerFrame)
        var mono = [Float](repeating: 0, count: frames)
        let bytes = UnsafeRawPointer(dataPointer)

        for frame in 0..<frames {
            var value: Float = 0
            for channel in 0..<channels {
                let sampleIndex = isNonInterleaved ? frame : frame * channels + channel
                let offset = isNonInterleaved ? sampleIndex * bytesPerSample : frame * bytesPerFrame + channel * bytesPerSample
                guard offset + bytesPerSample <= length else { continue }
                if isFloat && bytesPerSample == 4 {
                    value += bytes.loadUnaligned(fromByteOffset: offset, as: Float.self)
                } else if bytesPerSample == 2 {
                    let raw = bytes.loadUnaligned(fromByteOffset: offset, as: Int16.self)
                    value += Float(isBigEndian ? Int16(bigEndian: raw) : Int16(littleEndian: raw)) / 32_768
                } else if bytesPerSample == 4 {
                    let raw = bytes.loadUnaligned(fromByteOffset: offset, as: Int32.self)
                    value += Float(isBigEndian ? Int32(bigEndian: raw) : Int32(littleEndian: raw)) / 2_147_483_648
                }
            }
            mono[frame] = max(-1, min(1, value / Float(channels)))
        }
        mono.withUnsafeBufferPointer { buffer in
            guard let base = buffer.baseAddress else { return }
            callback(base, Int32(buffer.count), context)
        }
    }
}

private let captureLock = NSLock()
private var activeCapture: OpenSystemAudio?
private var captureError = ""

@_cdecl("openflow_system_audio_start")
public func openflowSystemAudioStart(_ callback: AudioCallback?, _ context: UnsafeMutableRawPointer?) -> Int32 {
    guard let callback else { return -1 }
    captureLock.lock()
    defer { captureLock.unlock() }
    guard activeCapture == nil else { return -2 }
    let capture = OpenSystemAudio(callback: callback, context: context)
    do {
        try capture.start()
        activeCapture = capture
        captureError = ""
        return 0
    } catch {
        captureError = error.localizedDescription
        return -3
    }
}

@_cdecl("openflow_system_audio_stop")
public func openflowSystemAudioStop() {
    captureLock.lock()
    let capture = activeCapture
    activeCapture = nil
    captureLock.unlock()
    capture?.stop()
}

@_cdecl("openflow_system_audio_last_error")
public func openflowSystemAudioLastError(_ buffer: UnsafeMutablePointer<CChar>?, _ capacity: Int32) {
    guard let buffer, capacity > 0 else { return }
    captureLock.lock()
    let message = captureError
    captureLock.unlock()
    _ = message.withCString { source in strlcpy(buffer, source, Int(capacity)) }
}
