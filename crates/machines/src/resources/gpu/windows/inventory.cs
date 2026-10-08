// Executed on the measured Windows host (including SSH targets), not the UI host.
// DXGI_ADAPTER_DESC1 + D3DKMT physical queries: Microsoft public dxgi/d3dkmthk APIs.
// Hardware registry names come from CM_Open_DevNode_Key + NtQueryKey; only exact
// object-name equality joins a PNP device. No registry-layout or enumeration guesses.
using System;
using System.Collections.Generic;
using System.Globalization;
using System.Runtime.InteropServices;

namespace CrabdashGpu {
    public sealed class Adapter {
        public string id, pnp_id, name, vendor, capacity;
    }
    public sealed class Inventory {
        public bool available;
        public List<Adapter> adapters = new List<Adapter>();
    }
    public static class Probe {
        [StructLayout(LayoutKind.Sequential)] struct Luid { public uint low; public int high; }
        [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)] struct Desc {
            [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 128)] public string description;
            public uint vendor, device, subsystem, revision;
            public UIntPtr video, system, shared;
            public Luid luid;
            public uint flags;
        }
        [StructLayout(LayoutKind.Sequential)] struct Open { public Luid luid; public uint handle; }
        [StructLayout(LayoutKind.Sequential)] struct Close { public uint handle; }
        [StructLayout(LayoutKind.Sequential)] struct Query { public uint handle; public int type; public IntPtr data; public uint size; }
        [StructLayout(LayoutKind.Sequential)] struct Count { public uint count; }
        [StructLayout(LayoutKind.Sequential)] struct DeviceIds { public uint index, vendor, device, subvendor, subsystem, revision, bus; }
        [StructLayout(LayoutKind.Sequential)] struct Pnp { public uint index; public int type; public IntPtr dest, length; }
        // D3DKMT_ALIGN64 fields must retain their 8-byte offsets even in 32-bit PowerShell.
        [StructLayout(LayoutKind.Explicit, Size = 56)] struct Segments {
            [FieldOffset(0)] public uint index;
            [FieldOffset(8)] public ulong video;
            [FieldOffset(16)] public ulong system;
            [FieldOffset(24)] public ulong shared;
            [FieldOffset(32)] public ulong local;
            [FieldOffset(40)] public ulong nonlocal;
            [FieldOffset(48)] public ulong nonbudget;
        }
        [UnmanagedFunctionPointer(CallingConvention.StdCall)] delegate int Enumerate(IntPtr self, uint index, out IntPtr adapter);
        [UnmanagedFunctionPointer(CallingConvention.StdCall)] delegate int Describe(IntPtr self, out Desc desc);
        [DllImport("dxgi.dll", ExactSpelling = true)] static extern int CreateDXGIFactory1(ref Guid iid, out IntPtr factory);
        [DllImport("gdi32.dll", ExactSpelling = true)] static extern int D3DKMTOpenAdapterFromLuid(ref Open request);
        [DllImport("gdi32.dll", ExactSpelling = true)] static extern int D3DKMTCloseAdapter(ref Close request);
        [DllImport("gdi32.dll", ExactSpelling = true)] static extern int D3DKMTQueryAdapterInfo(ref Query request);
        [DllImport("cfgmgr32.dll", CharSet = CharSet.Unicode, ExactSpelling = true)] static extern uint CM_Locate_DevNodeW(out uint node, string id, uint flags);
        [DllImport("cfgmgr32.dll", ExactSpelling = true)] static extern uint CM_Open_DevNode_Key(uint node, uint access, uint profile, uint disposition, out IntPtr key, uint flags);
        [DllImport("advapi32.dll", ExactSpelling = true)] static extern int RegCloseKey(IntPtr key);
        [DllImport("ntdll.dll", ExactSpelling = true)] static extern int NtQueryKey(IntPtr key, int type, IntPtr data, uint size, out uint length);

        static T Method<T>(IntPtr instance, int slot) where T : class {
            IntPtr table = Marshal.ReadIntPtr(instance);
            return Marshal.GetDelegateForFunctionPointer(Marshal.ReadIntPtr(table, slot * IntPtr.Size), typeof(T)) as T;
        }
        static bool Read<T>(uint handle, int type, T input, out T output) where T : struct {
            int size = Marshal.SizeOf(typeof(T));
            IntPtr data = Marshal.AllocHGlobal(size);
            try {
                Marshal.StructureToPtr(input, data, false);
                Query query = new Query { handle = handle, type = type, data = data, size = (uint)size };
                if (D3DKMTQueryAdapterInfo(ref query) < 0) { output = default(T); return false; }
                output = (T)Marshal.PtrToStructure(data, typeof(T));
                return true;
            } finally { Marshal.FreeHGlobal(data); }
        }
        static string DeviceKey(string id) {
            uint node;
            IntPtr key;
            if (String.IsNullOrEmpty(id) || CM_Locate_DevNodeW(out node, id, 0) != 0 ||
                CM_Open_DevNode_Key(node, 1, 0, 1, out key, 0) != 0) return null;
            try {
                uint size;
                int status = NtQueryKey(key, 3, IntPtr.Zero, 0, out size);
                if ((status != unchecked((int)0xC0000023) && status != unchecked((int)0x80000005)) || size < 4 || size > 65540) return null;
                IntPtr data = Marshal.AllocHGlobal((int)size);
                try {
                    uint actual;
                    if (NtQueryKey(key, 3, data, size, out actual) != 0 || actual < 4 || actual > size) return null;
                    uint bytes = unchecked((uint)Marshal.ReadInt32(data));
                    if ((bytes & 1) != 0 || bytes > actual - 4) return null;
                    return Marshal.PtrToStringUni(IntPtr.Add(data, 4), (int)(bytes / 2));
                } finally { Marshal.FreeHGlobal(data); }
            } finally { RegCloseKey(key); }
        }
        static string PhysicalKey(uint handle, uint index) {
            const int capacity = 32768;
            IntPtr data = Marshal.AllocHGlobal(capacity * 2);
            IntPtr length = Marshal.AllocHGlobal(4);
            try {
                Marshal.WriteInt32(length, capacity);
                Pnp value;
                if (!Read(handle, 41, new Pnp { index = index, type = 1, dest = data, length = length }, out value)) return null;
                int count = Marshal.ReadInt32(length);
                if (count < 1 || count > capacity || Marshal.ReadInt16(data, (count - 1) * 2) != 0) return null;
                return Marshal.PtrToStringUni(data, count - 1);
            } finally { Marshal.FreeHGlobal(length); Marshal.FreeHGlobal(data); }
        }
        static string Vendor(uint value) {
            switch (value) { case 0: return "Unknown"; case 0x1002: return "AMD"; case 0x10DE: return "NVIDIA"; case 0x8086: return "Intel"; default: return "Vendor 0x" + value.ToString("x", CultureInfo.InvariantCulture); }
        }
        public static Inventory Collect(string[] ids) {
            Inventory result = new Inventory();
            bool complete = true;
            Dictionary<string, string> keys = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase);
            foreach (string id in ids) {
                try { string key = DeviceKey(id); if (!String.IsNullOrEmpty(key)) {
                    string existing;
                    if (keys.TryGetValue(key, out existing) && !String.Equals(existing, id, StringComparison.OrdinalIgnoreCase)) keys[key] = null;
                    else if (!keys.ContainsKey(key)) keys[key] = id;
                } } catch { }
            }
            IntPtr factory = IntPtr.Zero;
            try {
                Guid iid = new Guid("770aae78-f26f-4dba-a829-253c83d1b387"); // IDXGIFactory1
                if (CreateDXGIFactory1(ref iid, out factory) < 0 || factory == IntPtr.Zero) return result;
                Enumerate enumerate = Method<Enumerate>(factory, 12);
                for (uint number = 0; number < 128; number++) {
                    IntPtr adapter;
                    int status = enumerate(factory, number, out adapter);
                    if (status == unchecked((int)0x887A0002)) { result.available = complete; break; } // DXGI_ERROR_NOT_FOUND
                    if (status < 0 || adapter == IntPtr.Zero) {
                        if (adapter != IntPtr.Zero) Marshal.Release(adapter);
                        break;
                    }
                    try {
                        Desc desc;
                        if (Method<Describe>(adapter, 10)(adapter, out desc) < 0) { complete = false; continue; }
                        if ((desc.flags & 2) != 0) continue;
                        Open open = new Open { luid = desc.luid };
                        if (D3DKMTOpenAdapterFromLuid(ref open) < 0) { complete = false; continue; }
                        try {
                            Count count;
                            if (!Read(open.handle, 30, new Count(), out count) || count.count == 0 || count.count > 64) { complete = false; continue; }
                            for (uint physical = 0; physical < count.count; physical++) {
                                DeviceIds device;
                                string vendor = null;
                                if (Read(open.handle, 31, new DeviceIds { index = physical }, out device)) vendor = Vendor(device.vendor);
                                else if (count.count == 1) vendor = Vendor(desc.vendor);
                                string pnp = null;
                                string key = PhysicalKey(open.handle, physical);
                                if (key != null) keys.TryGetValue(key, out pnp);
                                Segments segments;
                                string capacity = null;
                                try {
                                    if (Read(open.handle, 42, new Segments { index = physical }, out segments)) capacity = checked(segments.video + segments.system).ToString(CultureInfo.InvariantCulture);
                                    else if (count.count == 1) capacity = checked(desc.video.ToUInt64() + desc.system.ToUInt64()).ToString(CultureInfo.InvariantCulture);
                                } catch (OverflowException) { }
                                result.adapters.Add(new Adapter {
                                    id = "luid_0x" + unchecked((uint)desc.luid.high).ToString("x", CultureInfo.InvariantCulture) + "_0x" + desc.luid.low.ToString("x", CultureInfo.InvariantCulture) + "_phys_" + physical.ToString(CultureInfo.InvariantCulture),
                                    pnp_id = pnp, name = count.count == 1 ? desc.description : null,
                                    vendor = vendor, capacity = capacity
                                });
                            }
                        } finally { Close close = new Close { handle = open.handle }; D3DKMTCloseAdapter(ref close); }
                    } finally { Marshal.Release(adapter); }
                }
            } catch { } finally { if (factory != IntPtr.Zero) Marshal.Release(factory); }
            return result;
        }
    }
}
