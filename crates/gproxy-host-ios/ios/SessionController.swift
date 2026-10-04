import BackgroundTasks
import Combine
import UIKit

@MainActor
final class SessionController: ObservableObject {
    static let shared = SessionController()
    static let taskPrefix = "com.leenhawk.gproxy.ios.proxy-session"

    @Published private(set) var active = false
    @Published private(set) var busy = false
    @Published private(set) var canStop = false
    @Published private(set) var message = "会话未启动"
    @Published private(set) var baseURL = "http://127.0.0.1:8787"
    @Published private(set) var requests: UInt64 = 0
    @Published private(set) var apiKey = ""
    @Published private(set) var password = ""
    @Published private(set) var deadline: Date?

    private let bridge = NativeBridge()
    private var registered = false
    private var identifier: String?
    private var backgroundTask: BGContinuedProcessingTask?
    private var options: StartOptions?
    private var monitor: Task<Void, Never>?

    func register() {
        registered = BGTaskScheduler.shared.register(forTaskWithIdentifier: Self.taskPrefix + ".*", using: .main) { task in
            DispatchQueue.main.async { Self.shared.launch(task) }
        }
        if !registered { message = "后台任务注册失败" }
    }

    // Called only by the user's foreground Start button. No automatic restart.
    func begin(duration: Int) {
        guard !busy, !active, identifier == nil else { return }
        guard registered, UIApplication.shared.applicationState == .active else {
            message = "请在前台启动会话；后台任务尚未就绪"
            return
        }
        do {
            options = try Credentials.options(duration: duration)
            let id = Self.taskPrefix + "." + UUID().uuidString
            identifier = id
            busy = true
            canStop = true
            message = "正在申请后台执行并启动代理"
            let request = BGContinuedProcessingTaskRequest(identifier: id, title: "GPROXY 代理会话", subtitle: "正在启动")
            request.strategy = .fail
            try BGTaskScheduler.shared.submit(request)
        } catch {
            identifier = nil
            options = nil
            busy = false
            canStop = false
            message = error.localizedDescription
        }
    }

    private func launch(_ task: BGTask) {
        guard let task = task as? BGContinuedProcessingTask,
              task.identifier == identifier, let options else {
            task.setTaskCompleted(success: false)
            return
        }
        backgroundTask = task
        let id = task.identifier
        task.expirationHandler = {
            DispatchQueue.main.async {
                guard Self.shared.identifier == id else { return }
                Self.shared.stop(reason: "系统或用户结束了后台任务", success: false)
            }
        }
        // The request count is real; idle time is not fabricated progress.
        // An indeterminate workload can be expired by the system at any time.
        task.progress.totalUnitCount = -1
        Task {
            do {
                let status = try await bridge.call("start", input: options)
                guard identifier == id else { return }
                guard status.running else { throw failure("代理未启动") }
                baseURL = status.baseUrl ?? baseURL
                apiKey = options.apiKey
                password = options.password
                requests = status.requests
                deadline = Date().addingTimeInterval(TimeInterval(options.durationSeconds))
                active = true
                busy = false
                message = "会话运行中"
                task.updateTitle("GPROXY 代理会话", subtitle: "等待客户端请求")
                monitor = Task {
                    while !Task.isCancelled {
                        try? await Task.sleep(for: .seconds(2))
                        guard !Task.isCancelled, identifier == id else { return }
                        await refresh(id: id)
                    }
                }
            } catch {
                guard identifier == id else { return }
                stop(reason: error.localizedDescription, success: false)
            }
        }
    }

    private func refresh(id: String) async {
        do {
            let status = try await bridge.call("status")
            guard identifier == id else { return }
            if let deadline, Date() >= deadline {
                stop(reason: "会话已达到时长上限", success: true)
                return
            }
            guard status.running else {
                stop(reason: "本地代理已经停止", success: false)
                return
            }
            if requests != status.requests {
                requests = status.requests
                backgroundTask?.progress.completedUnitCount = Int64(clamping: requests)
                backgroundTask?.updateTitle("GPROXY 代理会话", subtitle: "已收到 \(requests) 个代理请求")
            }
        } catch {
            guard identifier == id else { return }
            stop(reason: error.localizedDescription, success: false)
        }
    }

    func stop(reason: String = "会话已停止", success: Bool = true) {
        guard let id = identifier else { return }
        identifier = nil
        active = false
        busy = true
        canStop = false
        deadline = nil
        options = nil
        monitor?.cancel()
        monitor = nil
        let task = backgroundTask
        backgroundTask = nil
        BGTaskScheduler.shared.cancel(taskRequestWithIdentifier: id)
        Task {
            do {
                _ = try await bridge.call("stop")
                task?.setTaskCompleted(success: success)
                message = reason
            } catch {
                task?.setTaskCompleted(success: false)
                message = error.localizedDescription
            }
            busy = false
        }
    }
}
