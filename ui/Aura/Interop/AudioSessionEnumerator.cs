using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;

namespace Aura.Interop;

internal static class AudioSessionEnumerator
{
    public static HashSet<uint>? GetPidsWithAudio()
    {
        try
        {
            var hr = CoCreateInstance(
                CLSID_MMDeviceEnumerator, IntPtr.Zero, CLSCTX_ALL,
                IID_IMMDeviceEnumerator, out var enumeratorPtr);
            if (hr < 0 || enumeratorPtr == IntPtr.Zero)
                return null;

            var deviceEnumerator = (IMMDeviceEnumerator)Marshal.GetObjectForIUnknown(enumeratorPtr);
            Marshal.Release(enumeratorPtr);

            hr = deviceEnumerator.GetDefaultAudioEndpoint((int)EDataFlow.eRender, (int)ERole.eMultimedia, out var device);
            if (hr != 0 || device == null)
                return null;

            var mgrIid = typeof(IAudioSessionManager2).GUID;
            hr = device.Activate(ref mgrIid, (int)CLSCTX_ALL, IntPtr.Zero, out var mgrObj);
            Marshal.ReleaseComObject(device);
            if (hr != 0 || mgrObj == null)
                return null;

            var manager = (IAudioSessionManager2)mgrObj;
            hr = manager.GetAudioSessionEnumerator(out var sessionEnum);
            Marshal.ReleaseComObject(manager);
            if (hr != 0 || sessionEnum == null)
                return null;

            hr = sessionEnum.GetCount(out var count);
            if (hr != 0)
                return null;

            var pids = new HashSet<uint>();
            for (int i = 0; i < count; i++)
            {
                hr = sessionEnum.GetSession(i, out var session);
                if (hr != 0 || session == null) continue;

                hr = session.GetProcessId(out var pid);
                Marshal.ReleaseComObject(session);
                if (hr == 0 && pid > 0)
                    pids.Add(pid);
            }

            Marshal.ReleaseComObject(sessionEnum);
            return pids;
        }
        catch
        {
            return null;
        }
    }

    [DllImport("ole32.dll", ExactSpelling = true, PreserveSig = true)]
    private static extern int CoCreateInstance(
        [MarshalAs(UnmanagedType.LPStruct)] Guid rclsid,
        IntPtr pUnkOuter,
        uint dwClsCtx,
        [MarshalAs(UnmanagedType.LPStruct)] Guid riid,
        out IntPtr ppv);

    private static readonly Guid CLSID_MMDeviceEnumerator = new("BCDE0395-E52F-467C-8E3D-C4579291692E");
    private static readonly Guid IID_IMMDeviceEnumerator = new("A95664D2-9614-4F35-A746-DE8DB63617E6");
    private const uint CLSCTX_ALL = 23;

    private enum EDataFlow { eRender = 0, eCapture, eAll }
    private enum ERole { eConsole = 0, eMultimedia, eCommunications }
}

[Guid("A95664D2-9614-4F35-A746-DE8DB63617E6")]
[InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
internal interface IMMDeviceEnumerator
{
    [PreserveSig]
    int EnumAudioEndpoints(int dataFlow, int stateMask, out IntPtr devices);
    [PreserveSig]
    int GetDefaultAudioEndpoint(int dataFlow, int role, out IMMDevice device);
    [PreserveSig]
    int GetDevice([MarshalAs(UnmanagedType.LPWStr)] string id, out IntPtr device);
    [PreserveSig]
    int RegisterEndpointNotificationCallback(IntPtr client);
    [PreserveSig]
    int UnregisterEndpointNotificationCallback(IntPtr client);
}

[Guid("D666063F-1587-4E43-81F1-B948E807363F")]
[InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
internal interface IMMDevice
{
    [PreserveSig]
    int Activate(ref Guid iid, int dwClsCtx, IntPtr pActivationParams, [MarshalAs(UnmanagedType.IUnknown)] out object ppInterface);
    [PreserveSig]
    int OpenPropertyStore(int stgmAccess, out IntPtr properties);
    [PreserveSig]
    int GetId([MarshalAs(UnmanagedType.LPWStr)] out string id);
    [PreserveSig]
    int GetState(out int state);
}

[Guid("77AA99A0-1BD6-484F-8BC8-EF0F6CC3D269")]
[InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
internal interface IAudioSessionManager2
{
    // IAudioSessionManager methods (vtable slots 3-4)
    [PreserveSig]
    int GetAudioSessionControl(IntPtr audioSessionGuid, uint streamFlags, out IntPtr sessionControl);
    [PreserveSig]
    int GetSimpleAudioVolume(IntPtr audioSessionGuid, uint streamFlags, out IntPtr audioVolume);

    // IAudioSessionManager2 methods (vtable slots 5+)
    [PreserveSig]
    int GetAudioSessionEnumerator(out IAudioSessionEnumerator sessionEnum);
    [PreserveSig]
    int GetSessionIdentifier(IntPtr guid, [MarshalAs(UnmanagedType.LPWStr)] out string id);
    [PreserveSig]
    int RegisterSessionNotification(IntPtr notification);
    [PreserveSig]
    int UnregisterSessionNotification(IntPtr notification);
    [PreserveSig]
    int RegisterDuckNotification(IntPtr sessionId, IntPtr notification);
    [PreserveSig]
    int UnregisterDuckNotification(IntPtr notification);
}

[Guid("E2F5BB11-0570-40CA-ACDD-3AA01277DEE8")]
[InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
internal interface IAudioSessionEnumerator
{
    [PreserveSig]
    int GetCount(out int count);
    [PreserveSig]
    int GetSession(int sessionCount, [MarshalAs(UnmanagedType.Interface)] out IAudioSessionControl2 session);
}

[Guid("BFB7FF88-6799-4FA9-8C10-16F102E3712E")]
[InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
internal interface IAudioSessionControl2
{
    [PreserveSig]
    int GetState(out int state);
    [PreserveSig]
    int GetDisplayName([MarshalAs(UnmanagedType.LPWStr)] out string displayName);
    [PreserveSig]
    int SetDisplayName([MarshalAs(UnmanagedType.LPWStr)] string value, ref Guid eventContext);
    [PreserveSig]
    int GetIconPath([MarshalAs(UnmanagedType.LPWStr)] out string iconPath);
    [PreserveSig]
    int SetIconPath([MarshalAs(UnmanagedType.LPWStr)] string value, ref Guid eventContext);
    [PreserveSig]
    int GetGroupingParam(out Guid groupingParam);
    [PreserveSig]
    int SetGroupingParam(ref Guid groupingParam, ref Guid eventContext);
    [PreserveSig]
    int GetSessionIdentifier([MarshalAs(UnmanagedType.LPWStr)] out string sessionId);
    [PreserveSig]
    int GetSessionInstanceIdentifier([MarshalAs(UnmanagedType.LPWStr)] out string sessionInstanceId);
    [PreserveSig]
    int GetProcessId(out uint pid);
    [PreserveSig]
    int IsSystemSoundsSession();
    [PreserveSig]
    int SetDuckingPreference([MarshalAs(UnmanagedType.Bool)] bool optOut);
}
