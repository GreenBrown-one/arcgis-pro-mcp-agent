using System.Diagnostics;
using ArcGIS.Desktop.Framework;
using ArcGIS.Desktop.Framework.Contracts;
using ArcGIS.Desktop.Framework.Dialogs;
using ArcGISProAgent.Bridge;
using ArcGISProAgent.Contracts;
using ArcGisEventLog = ArcGIS.Desktop.Framework.Utilities.EventLog;

namespace ArcGISProAgent.AddIn;

internal sealed class AgentModule : Module
{
    private const string ModuleId = "ArcGISProAgent_AddIn_Module";

    private NamedPipeBridgeServer? _server;
    private CancellationTokenSource? _cts;
    private Task? _runTask;

    internal static AgentModule Current =>
        (AgentModule)FrameworkApplication.FindModule(ModuleId);

    protected override bool Initialize()
    {
        _cts = new CancellationTokenSource();
        _server = new NamedPipeBridgeServer(
            BridgeProtocol.DefaultPipeName,
            () => RuntimeCredentials.Load(RuntimeCredentialLocator.GetPath()).Token);
        var dispatcher = new ArcGisOperationDispatcher();
        _runTask = ObserveRunTaskAsync(
            _server.RunAsync(dispatcher.DispatchAsync, _cts.Token),
            _cts.Token);
        return true;
    }

    protected override bool CanUnload()
    {
        _cts?.Cancel();
        _cts?.Dispose();
        _cts = null;
        _server = null;
        return true;
    }

    internal string GetStatusDetails()
    {
        var listening = _runTask is { IsCompleted: false };
        var addInVersion = typeof(AgentModule).Assembly.GetName().Version?.ToString() ?? "0.2.0-preview.1";
        var processPath = Environment.ProcessPath;
        var arcGisVersion = processPath is null
            ? "unknown"
            : FileVersionInfo.GetVersionInfo(processPath).ProductVersion ?? "unknown";

        return $"桥接：{(listening ? "正在监听" : "未运行")}\n"
            + $"管道：{BridgeProtocol.DefaultPipeName}\n"
            + $"协议版本：{BridgeProtocol.Current}\n"
            + $"Add-In 版本：{addInVersion}\n"
            + $"ArcGIS Pro 版本：{arcGisVersion}";
    }

    private static async Task ObserveRunTaskAsync(Task runTask, CancellationToken ct)
    {
        try
        {
            await runTask.ConfigureAwait(false);
        }
        catch (OperationCanceledException) when (ct.IsCancellationRequested)
        {
            ArcGisEventLog.Write(
                ArcGisEventLog.EventType.Information,
                "ArcGIS Pro Agent bridge stopped after cancellation.",
                flush: true);
        }
        catch (Exception ex)
        {
            ArcGisEventLog.Write(
                ArcGisEventLog.EventType.Error,
                $"ArcGIS Pro Agent bridge stopped unexpectedly: {ex.GetType().Name}: {ex.Message}",
                flush: true);
        }
    }
}

internal sealed class StatusButton : Button
{
    protected override void OnClick() =>
        MessageBox.Show(
            AgentModule.Current.GetStatusDetails(),
            "ArcGIS Pro 智能助手桥接");
}
