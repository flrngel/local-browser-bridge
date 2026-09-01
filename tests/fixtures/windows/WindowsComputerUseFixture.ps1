#requires -Version 5.1

[CmdletBinding(DefaultParameterSetName = "Run")]
param(
    [Parameter(ParameterSetName = "Run", Mandatory = $true)]
    [ValidateNotNullOrEmpty()]
    [string]$EvidenceDirectory,

    [Parameter(ParameterSetName = "Run")]
    [switch]$ShowOccluder,

    [Parameter(ParameterSetName = "Build", Mandatory = $true)]
    [ValidateNotNullOrEmpty()]
    [string]$BuildExecutablePath,

    [Parameter(ParameterSetName = "Build", Mandatory = $true)]
    [ValidatePattern('^[0-9a-f]{64}$')]
    [string]$ExpectedSourceSha256,

    [Parameter(ParameterSetName = "SelfTest", Mandatory = $true)]
    [switch]$SelfTest
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Get-FixtureSourceSha256 {
    param([string]$Path)
    $stream = [IO.File]::OpenRead([IO.Path]::GetFullPath($Path))
    $sha256 = [Security.Cryptography.SHA256]::Create()
    try {
        return (($sha256.ComputeHash($stream) | ForEach-Object { $_.ToString("x2") }) -join '')
    }
    finally {
        $sha256.Dispose()
        $stream.Dispose()
    }
}

if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) {
    throw "The Windows computer-use fixture can run only on Windows."
}

$evidenceRoot = $null
if ($PSCmdlet.ParameterSetName -eq "Run") {
    $evidenceRoot = [IO.Path]::GetFullPath($EvidenceDirectory)
    [IO.Directory]::CreateDirectory($evidenceRoot) | Out-Null

    foreach ($name in @("fixture-state.json", "fixture-events.ndjson", "fixture-ready.json")) {
        $path = [IO.Path]::Combine($evidenceRoot, $name)
        if ([IO.File]::Exists($path)) {
            throw "The evidence directory already contains $name. Supply a new, empty directory."
        }
    }
}

$fixtureSource = @'
using System;
using System.Collections.Generic;
using System.Drawing;
using System.Globalization;
using System.IO;
using System.Runtime.InteropServices;
using System.Security.Cryptography;
using System.Text;
using System.Threading;
using System.Windows.Forms;

namespace LbbWindowsFixture
{
    internal static class JsonValueWriter
    {
        internal static string Serialize(IDictionary<string, object> value)
        {
            if (value == null)
            {
                throw new ArgumentNullException("value");
            }
            StringBuilder output = new StringBuilder();
            AppendObject(output, value);
            return output.ToString();
        }

        private static void AppendObject(StringBuilder output, IDictionary<string, object> value)
        {
            output.Append('{');
            bool first = true;
            foreach (KeyValuePair<string, object> item in value)
            {
                if (!first)
                {
                    output.Append(',');
                }
                first = false;
                AppendString(output, item.Key);
                output.Append(':');
                AppendValue(output, item.Value);
            }
            output.Append('}');
        }

        private static void AppendValue(StringBuilder output, object value)
        {
            if (value == null)
            {
                output.Append("null");
                return;
            }
            string text = value as string;
            if (text != null)
            {
                AppendString(output, text);
                return;
            }
            if (value is bool)
            {
                output.Append((bool)value ? "true" : "false");
                return;
            }
            IDictionary<string, object> dictionary = value as IDictionary<string, object>;
            if (dictionary != null)
            {
                AppendObject(output, dictionary);
                return;
            }
            if ((value is double && (Double.IsNaN((double)value) || Double.IsInfinity((double)value))) ||
                (value is float && (Single.IsNaN((float)value) || Single.IsInfinity((float)value))))
            {
                throw new InvalidOperationException("Non-finite fixture JSON numbers are forbidden.");
            }
            TypeCode typeCode = Type.GetTypeCode(value.GetType());
            if (typeCode == TypeCode.Byte || typeCode == TypeCode.SByte ||
                typeCode == TypeCode.Int16 || typeCode == TypeCode.UInt16 ||
                typeCode == TypeCode.Int32 || typeCode == TypeCode.UInt32 ||
                typeCode == TypeCode.Int64 || typeCode == TypeCode.UInt64 ||
                typeCode == TypeCode.Decimal || typeCode == TypeCode.Double ||
                typeCode == TypeCode.Single)
            {
                output.Append(Convert.ToString(value, CultureInfo.InvariantCulture));
                return;
            }
            throw new InvalidOperationException("Unsupported fixture JSON value type: " + value.GetType().FullName);
        }

        private static void AppendString(StringBuilder output, string value)
        {
            output.Append('"');
            foreach (char character in value)
            {
                switch (character)
                {
                    case '"': output.Append("\\\""); break;
                    case '\\': output.Append("\\\\"); break;
                    case '\b': output.Append("\\b"); break;
                    case '\f': output.Append("\\f"); break;
                    case '\n': output.Append("\\n"); break;
                    case '\r': output.Append("\\r"); break;
                    case '\t': output.Append("\\t"); break;
                    default:
                        if (character < 0x20)
                        {
                            output.Append("\\u");
                            output.Append(((int)character).ToString("x4", CultureInfo.InvariantCulture));
                        }
                        else
                        {
                            output.Append(character);
                        }
                        break;
                }
            }
            output.Append('"');
        }
    }

    internal sealed class EvidenceStore
    {
        private readonly object gate = new object();
        private readonly string statePath;
        private readonly string eventPath;
        private readonly string readyPath;
        private long sequence;

        internal EvidenceStore(string directory)
        {
            statePath = Path.Combine(directory, "fixture-state.json");
            eventPath = Path.Combine(directory, "fixture-events.ndjson");
            readyPath = Path.Combine(directory, "fixture-ready.json");
        }

        internal long AppendEvent(string source, string eventName, IDictionary<string, object> details)
        {
            lock (gate)
            {
                long current = ++sequence;
                Dictionary<string, object> record = new Dictionary<string, object>();
                record["schemaVersion"] = 1;
                record["sequence"] = current;
                record["utc"] = DateTime.UtcNow.ToString("o", CultureInfo.InvariantCulture);
                record["source"] = source;
                record["event"] = eventName;
                if (details != null)
                {
                    foreach (KeyValuePair<string, object> item in details)
                    {
                        record[item.Key] = item.Value;
                    }
                }
                File.AppendAllText(eventPath, JsonValueWriter.Serialize(record) + Environment.NewLine, new UTF8Encoding(false));
                return current;
            }
        }

        internal long CurrentSequence
        {
            get
            {
                lock (gate)
                {
                    return sequence;
                }
            }
        }

        internal void WriteState(IDictionary<string, object> state)
        {
            lock (gate)
            {
                File.WriteAllText(statePath, JsonValueWriter.Serialize(state), new UTF8Encoding(false));
            }
        }

        internal void WriteReady(IDictionary<string, object> ready)
        {
            lock (gate)
            {
                File.WriteAllText(readyPath, JsonValueWriter.Serialize(ready), new UTF8Encoding(false));
            }
        }

        internal static string HashText(string value)
        {
            using (SHA256 sha = SHA256.Create())
            {
                byte[] hash = sha.ComputeHash(Encoding.UTF8.GetBytes(value ?? String.Empty));
                StringBuilder output = new StringBuilder(hash.Length * 2);
                foreach (byte item in hash)
                {
                    output.Append(item.ToString("x2", CultureInfo.InvariantCulture));
                }
                return output.ToString();
            }
        }
    }

    internal sealed class MessageCounters
    {
        internal int MouseMove;
        internal int MouseDown;
        internal int MouseUp;
        internal int MouseDoubleClick;
        internal int DragMove;
        internal int MouseWheel;
        internal int MouseHWheel;
        internal int KeyDown;
        internal int KeyUp;
        internal int Char;
        internal int SysKeyDown;
        internal int SysKeyUp;
        internal int SysChar;
        internal int WheelDelta;
        internal int HWheelDelta;
        internal long LastSequence;

        internal IDictionary<string, object> Snapshot()
        {
            Dictionary<string, object> output = new Dictionary<string, object>();
            output["mouseMove"] = MouseMove;
            output["mouseDown"] = MouseDown;
            output["mouseUp"] = MouseUp;
            output["mouseDoubleClick"] = MouseDoubleClick;
            output["dragMove"] = DragMove;
            output["mouseWheel"] = MouseWheel;
            output["mouseHWheel"] = MouseHWheel;
            output["keyDown"] = KeyDown;
            output["keyUp"] = KeyUp;
            output["char"] = Char;
            output["sysKeyDown"] = SysKeyDown;
            output["sysKeyUp"] = SysKeyUp;
            output["sysChar"] = SysChar;
            output["wheelDelta"] = WheelDelta;
            output["hWheelDelta"] = HWheelDelta;
            output["lastEventSequence"] = LastSequence;
            return output;
        }
    }

    internal abstract class LoggingControl : Control
    {
        protected const int WM_MOUSEMOVE = 0x0200;
        protected const int WM_LBUTTONDOWN = 0x0201;
        protected const int WM_LBUTTONUP = 0x0202;
        protected const int WM_LBUTTONDBLCLK = 0x0203;
        protected const int WM_RBUTTONDOWN = 0x0204;
        protected const int WM_RBUTTONUP = 0x0205;
        protected const int WM_RBUTTONDBLCLK = 0x0206;
        protected const int WM_MBUTTONDOWN = 0x0207;
        protected const int WM_MBUTTONUP = 0x0208;
        protected const int WM_MBUTTONDBLCLK = 0x0209;
        protected const int WM_MOUSEWHEEL = 0x020A;
        protected const int WM_MOUSEHWHEEL = 0x020E;
        protected const int WM_KEYDOWN = 0x0100;
        protected const int WM_KEYUP = 0x0101;
        protected const int WM_CHAR = 0x0102;
        protected const int WM_SYSKEYDOWN = 0x0104;
        protected const int WM_SYSKEYUP = 0x0105;
        protected const int WM_SYSCHAR = 0x0106;

        protected readonly EvidenceStore Store;
        protected readonly MessageCounters Counters;
        protected readonly string SourceName;
        private bool leftPressed;

        protected LoggingControl(EvidenceStore store, MessageCounters counters, string sourceName)
        {
            Store = store;
            Counters = counters;
            SourceName = sourceName;
            SetStyle(ControlStyles.StandardClick | ControlStyles.StandardDoubleClick | ControlStyles.UserPaint, true);
        }

        protected bool RecordMessage(ref Message message)
        {
            int id = message.Msg;
            string eventName = null;
            if (id == WM_MOUSEMOVE)
            {
                Counters.MouseMove++;
                if (leftPressed)
                {
                    Counters.DragMove++;
                    eventName = "dragMove";
                }
                else
                {
                    eventName = "mouseMove";
                }
            }
            else if (id == WM_LBUTTONDOWN || id == WM_RBUTTONDOWN || id == WM_MBUTTONDOWN)
            {
                Counters.MouseDown++;
                if (id == WM_LBUTTONDOWN)
                {
                    leftPressed = true;
                }
                eventName = "mouseDown";
            }
            else if (id == WM_LBUTTONUP || id == WM_RBUTTONUP || id == WM_MBUTTONUP)
            {
                Counters.MouseUp++;
                if (id == WM_LBUTTONUP)
                {
                    leftPressed = false;
                }
                eventName = "mouseUp";
            }
            else if (id == WM_LBUTTONDBLCLK || id == WM_RBUTTONDBLCLK || id == WM_MBUTTONDBLCLK)
            {
                Counters.MouseDoubleClick++;
                if (id == WM_LBUTTONDBLCLK)
                {
                    leftPressed = true;
                }
                eventName = "mouseDoubleClick";
            }
            else if (id == WM_MOUSEWHEEL)
            {
                Counters.MouseWheel++;
                Counters.WheelDelta += SignedHighWord(message.WParam.ToInt64());
                eventName = "mouseWheel";
            }
            else if (id == WM_MOUSEHWHEEL)
            {
                Counters.MouseHWheel++;
                Counters.HWheelDelta += SignedHighWord(message.WParam.ToInt64());
                eventName = "mouseHWheel";
            }
            else if (id == WM_KEYDOWN)
            {
                Counters.KeyDown++;
                eventName = "keyDown";
            }
            else if (id == WM_KEYUP)
            {
                Counters.KeyUp++;
                eventName = "keyUp";
            }
            else if (id == WM_CHAR)
            {
                Counters.Char++;
                eventName = "char";
            }
            else if (id == WM_SYSKEYDOWN)
            {
                Counters.SysKeyDown++;
                eventName = "sysKeyDown";
            }
            else if (id == WM_SYSKEYUP)
            {
                Counters.SysKeyUp++;
                eventName = "sysKeyUp";
            }
            else if (id == WM_SYSCHAR)
            {
                Counters.SysChar++;
                eventName = "sysChar";
            }

            if (eventName == null)
            {
                return false;
            }

            long rawWParam = message.WParam.ToInt64();
            long rawLParam = message.LParam.ToInt64();
            uint lowLParam = unchecked((uint)rawLParam);
            Dictionary<string, object> details = new Dictionary<string, object>();
            details["messageId"] = "0x" + id.ToString("X4", CultureInfo.InvariantCulture);
            bool characterMessage = id == WM_CHAR || id == WM_SYSCHAR;
            details["wParam"] = characterMessage ? (object)"[REDACTED_CHARACTER]" : rawWParam.ToString(CultureInfo.InvariantCulture);
            if (characterMessage)
            {
                details["characterRedacted"] = true;
            }
            details["lParam"] = rawLParam.ToString(CultureInfo.InvariantCulture);
            details["lParamHex"] = "0x" + lowLParam.ToString("X8", CultureInfo.InvariantCulture);
            details["x"] = unchecked((short)(lowLParam & 0xffff));
            details["y"] = unchecked((short)((lowLParam >> 16) & 0xffff));
            if (id >= WM_KEYDOWN && id <= WM_SYSCHAR)
            {
                details["repeatCount"] = lowLParam & 0xffff;
                details["scanCode"] = (lowLParam >> 16) & 0xff;
                details["extended"] = ((lowLParam >> 24) & 1) != 0;
                details["altContext"] = ((lowLParam >> 29) & 1) != 0;
                details["previousState"] = ((lowLParam >> 30) & 1) != 0;
                details["transitionState"] = ((lowLParam >> 31) & 1) != 0;
            }
            if (id == WM_MOUSEWHEEL || id == WM_MOUSEHWHEEL)
            {
                details["wheelDelta"] = SignedHighWord(rawWParam);
            }
            Counters.LastSequence = Store.AppendEvent(SourceName, eventName, details);
            return true;
        }

        private static int SignedHighWord(long value)
        {
            return unchecked((short)((value >> 16) & 0xffff));
        }
    }

    internal sealed class InputSurface : LoggingControl
    {
        internal InputSurface(EvidenceStore store, MessageCounters counters)
            : base(store, counters, "inputSurface")
        {
            AccessibleName = "Pixel Input Surface";
            AccessibleDescription = "Fixture-owned surface for exact-window mouse and key message verification";
            TabStop = true;
            BackColor = Color.FromArgb(22, 32, 49);
            ForeColor = Color.White;
            DoubleBuffered = true;
        }

        protected override void WndProc(ref Message message)
        {
            RecordMessage(ref message);
            base.WndProc(ref message);
            if (message.Msg == WM_LBUTTONDOWN || message.Msg == WM_RBUTTONDOWN || message.Msg == WM_MBUTTONDOWN)
            {
                Focus();
            }
        }

        protected override void OnPaint(PaintEventArgs eventArgs)
        {
            base.OnPaint(eventArgs);
            using (Pen border = new Pen(Color.FromArgb(62, 207, 255), 3.0f))
            using (Brush title = new SolidBrush(Color.White))
            using (Brush detail = new SolidBrush(Color.FromArgb(179, 222, 235)))
            {
                eventArgs.Graphics.DrawRectangle(border, 6, 6, Width - 13, Height - 13);
                eventArgs.Graphics.DrawString("PIXEL INPUT SURFACE", new Font(Font, FontStyle.Bold), title, 24, 24);
                eventArgs.Graphics.DrawString("move / click / double-click / drag / wheel", Font, detail, 24, 58);
                eventArgs.Graphics.DrawLine(border, 24, Height - 45, Width - 24, Height - 45);
                eventArgs.Graphics.DrawEllipse(border, Width / 2 - 15, Height / 2 - 15, 30, 30);
            }
        }
    }

    internal sealed class LoggingTextBox : TextBox
    {
        private readonly EvidenceStore store;
        private readonly MessageCounters counters;

        internal LoggingTextBox(EvidenceStore evidenceStore, MessageCounters messageCounters)
        {
            store = evidenceStore;
            counters = messageCounters;
        }

        protected override void WndProc(ref Message message)
        {
            int id = message.Msg;
            string eventName = null;
            if (id == 0x0100) { counters.KeyDown++; eventName = "keyDown"; }
            else if (id == 0x0101) { counters.KeyUp++; eventName = "keyUp"; }
            else if (id == 0x0102) { counters.Char++; eventName = "char"; }
            else if (id == 0x0104) { counters.SysKeyDown++; eventName = "sysKeyDown"; }
            else if (id == 0x0105) { counters.SysKeyUp++; eventName = "sysKeyUp"; }
            else if (id == 0x0106) { counters.SysChar++; eventName = "sysChar"; }
            if (eventName != null)
            {
                long rawLParam = message.LParam.ToInt64();
                uint low = unchecked((uint)rawLParam);
                Dictionary<string, object> details = new Dictionary<string, object>();
                details["messageId"] = "0x" + id.ToString("X4", CultureInfo.InvariantCulture);
                bool characterMessage = id == 0x0102 || id == 0x0106;
                details["wParam"] = characterMessage ? (object)"[REDACTED_CHARACTER]" : message.WParam.ToInt64().ToString(CultureInfo.InvariantCulture);
                if (characterMessage)
                {
                    details["characterRedacted"] = true;
                }
                details["lParam"] = rawLParam.ToString(CultureInfo.InvariantCulture);
                details["lParamHex"] = "0x" + low.ToString("X8", CultureInfo.InvariantCulture);
                details["repeatCount"] = low & 0xffff;
                details["scanCode"] = (low >> 16) & 0xff;
                details["extended"] = ((low >> 24) & 1) != 0;
                details["altContext"] = ((low >> 29) & 1) != 0;
                details["previousState"] = ((low >> 30) & 1) != 0;
                details["transitionState"] = ((low >> 31) & 1) != 0;
                counters.LastSequence = store.AppendEvent("focusedTextInput", eventName, details);
            }
            base.WndProc(ref message);
        }
    }

    internal sealed class AnimatedTargetPanel : Panel
    {
        private int frame;

        internal AnimatedTargetPanel()
        {
            DoubleBuffered = true;
            BackColor = Color.FromArgb(8, 20, 37);
            AccessibleName = "Animated Capture Target";
        }

        internal int Frame
        {
            get { return frame; }
            set { frame = value; Invalidate(); }
        }

        protected override void OnPaint(PaintEventArgs eventArgs)
        {
            base.OnPaint(eventArgs);
            int travel = Math.Max(1, Width - 110);
            int x = 25 + ((frame * 13) % travel);
            using (Brush heading = new SolidBrush(Color.White))
            using (Brush accent = new SolidBrush(Color.FromArgb(43, 220, 184)))
            using (Pen track = new Pen(Color.FromArgb(82, 112, 143), 2.0f))
            {
                eventArgs.Graphics.DrawString("EXACT WINDOW CAPTURE FIXTURE", new Font(Font.FontFamily, 16.0f, FontStyle.Bold), heading, 20, 15);
                eventArgs.Graphics.DrawLine(track, 25, Height - 31, Width - 25, Height - 31);
                eventArgs.Graphics.FillRectangle(accent, x, Height - 54, 44, 44);
                eventArgs.Graphics.DrawString("frame " + frame.ToString(CultureInfo.InvariantCulture), Font, heading, Width - 105, 20);
            }
        }
    }

    internal sealed class BackdropForm : Form
    {
        protected override bool ShowWithoutActivation
        {
            get { return true; }
        }

        protected override CreateParams CreateParams
        {
            get
            {
                const int WS_EX_NOACTIVATE = 0x08000000;
                const int WS_EX_TOOLWINDOW = 0x00000080;
                CreateParams parameters = base.CreateParams;
                parameters.ExStyle |= WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW;
                return parameters;
            }
        }

        internal BackdropForm(Rectangle targetBounds)
        {
            Text = "LBB Sanitized Capture Backdrop";
            AccessibleName = "LBB Sanitized Capture Backdrop";
            FormBorderStyle = FormBorderStyle.None;
            ShowInTaskbar = false;
            StartPosition = FormStartPosition.Manual;
            Bounds = Rectangle.Inflate(targetBounds, 28, 28);
            TopMost = true;
            BackColor = Color.FromArgb(16, 24, 32);
        }
    }

    internal sealed class TargetForm : Form
    {
        private const int SW_SHOWNOACTIVATE = 4;

        [DllImport("user32.dll")]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool ShowWindow(IntPtr window, int command);

        protected override bool ShowWithoutActivation
        {
            get { return true; }
        }

        protected override CreateParams CreateParams
        {
            get
            {
                const int WS_EX_NOACTIVATE = 0x08000000;
                CreateParams parameters = base.CreateParams;
                parameters.ExStyle |= WS_EX_NOACTIVATE;
                return parameters;
            }
        }

        private readonly EvidenceStore store;
        private readonly MessageCounters counters;
        private readonly AnimatedTargetPanel animation;
        private readonly TextBox semanticValue;
        private readonly TextBox invokeResult;
        private readonly LoggingTextBox focusedText;
        private readonly Label counterLabel;
        private readonly InputSurface surface;
        private readonly System.Windows.Forms.Timer timer;
        private readonly DateTime startedAt;
        private int animationFrame;
        private int invokeCount;
        private int activatedCount;
        private long statePublicationGeneration;
        private bool readyWritten;
        private bool lifecycleStarted;

        internal TargetForm(EvidenceStore evidenceStore, MessageCounters messageCounters)
        {
            store = evidenceStore;
            counters = messageCounters;
            startedAt = DateTime.UtcNow;
            Text = "LBB Windows Fixture Target";
            AccessibleName = "LBB Windows Fixture Target";
            StartPosition = FormStartPosition.Manual;
            Location = new Point(40, 60);
            ClientSize = new Size(820, 590);
            MinimumSize = new Size(700, 540);
            TopMost = true;
            AutoScaleMode = AutoScaleMode.Dpi;
            BackColor = Color.FromArgb(241, 246, 250);
            Font = new Font("Segoe UI", 10.0f, FontStyle.Regular, GraphicsUnit.Point);

            animation = new AnimatedTargetPanel();
            animation.Location = new Point(20, 18);
            animation.Size = new Size(780, 126);
            animation.Anchor = AnchorStyles.Top | AnchorStyles.Left | AnchorStyles.Right;
            Controls.Add(animation);

            Label semanticLabel = NewLabel("ValuePattern field", 20, 163, 145);
            semanticLabel.AccessibleName = "ValuePattern Field Label";
            Controls.Add(semanticLabel);
            semanticValue = new TextBox();
            semanticValue.Name = "SemanticValueInput";
            semanticValue.AccessibleName = "Fixture Value Input";
            semanticValue.AccessibleDescription = "A non-sensitive fixture value field supporting UI Automation ValuePattern";
            semanticValue.Text = "initial-value";
            semanticValue.Location = new Point(170, 158);
            semanticValue.Size = new Size(300, 30);
            semanticValue.Anchor = AnchorStyles.Top | AnchorStyles.Left | AnchorStyles.Right;
            Controls.Add(semanticValue);

            Button invokeButton = new Button();
            invokeButton.Name = "IncrementButton";
            invokeButton.AccessibleName = "Increment Counter";
            invokeButton.AccessibleDescription = "Increments the fixture-owned invocation counter through UI Automation InvokePattern";
            invokeButton.Text = "Increment counter";
            invokeButton.Location = new Point(500, 156);
            invokeButton.Size = new Size(150, 34);
            invokeButton.Anchor = AnchorStyles.Top | AnchorStyles.Right;
            invokeButton.Click += delegate
            {
                invokeCount++;
                counterLabel.Text = "Count: " + invokeCount.ToString(CultureInfo.InvariantCulture);
                invokeResult.Text = "invoke-count-" + invokeCount.ToString(CultureInfo.InvariantCulture);
                Dictionary<string, object> details = new Dictionary<string, object>();
                details["invokeCount"] = invokeCount;
                store.AppendEvent("invokeButton", "invoked", details);
                WriteState();
            };
            Controls.Add(invokeButton);

            counterLabel = NewLabel("Count: 0", 665, 163, 120);
            counterLabel.AccessibleName = "Invocation Counter";
            counterLabel.Anchor = AnchorStyles.Top | AnchorStyles.Right;
            Controls.Add(counterLabel);

            Label resultLabel = NewLabel("Invoke result", 20, 207, 145);
            Controls.Add(resultLabel);
            invokeResult = new TextBox();
            invokeResult.AccessibleName = "Invoke Result Value";
            invokeResult.Text = "invoke-count-0";
            invokeResult.Location = new Point(170, 202);
            invokeResult.Size = new Size(205, 30);
            Controls.Add(invokeResult);

            Label focusLabel = NewLabel("Focused text input", 400, 207, 145);
            Controls.Add(focusLabel);
            focusedText = new LoggingTextBox(store, counters);
            focusedText.Name = "FocusedTextInput";
            focusedText.AccessibleName = "Focused Text Input";
            focusedText.AccessibleDescription = "Fixture-owned text recipient for background input validation";
            focusedText.Text = String.Empty;
            focusedText.Location = new Point(550, 202);
            focusedText.Size = new Size(250, 30);
            focusedText.Anchor = AnchorStyles.Top | AnchorStyles.Left | AnchorStyles.Right;
            Controls.Add(focusedText);

            surface = new InputSurface(store, counters);
            surface.Location = new Point(20, 252);
            surface.Size = new Size(780, 285);
            surface.Anchor = AnchorStyles.Top | AnchorStyles.Bottom | AnchorStyles.Left | AnchorStyles.Right;
            Controls.Add(surface);

            Label footer = NewLabel("Fixture-owned content only. Raw key and mouse messages are logged without character contents.", 20, 553, 780);
            footer.ForeColor = Color.FromArgb(70, 84, 98);
            footer.Anchor = AnchorStyles.Bottom | AnchorStyles.Left | AnchorStyles.Right;
            Controls.Add(footer);

            timer = new System.Windows.Forms.Timer();
            timer.Interval = 100;
            timer.Tick += delegate
            {
                animationFrame++;
                animation.Frame = animationFrame;
                if ((animationFrame % 2) == 0)
                {
                    WriteState();
                }
                if (!readyWritten && FixtureRuntime.CompanionsReady)
                {
                    WriteReady();
                    readyWritten = true;
                }
            };

            FormClosed += delegate
            {
                timer.Stop();
                store.AppendEvent("target", "closed", null);
                WriteState();
                FixtureRuntime.StopCompanions();
            };
            semanticValue.TextChanged += delegate { WriteState(); };
            focusedText.TextChanged += delegate { WriteState(); };
        }

        internal void ShowNonActivating()
        {
            if (lifecycleStarted)
            {
                throw new InvalidOperationException("The fixture target can be shown only once.");
            }
            lifecycleStarted = true;
            IntPtr window = Handle;
            ShowWindow(window, SW_SHOWNOACTIVATE);
            timer.Start();
            store.AppendEvent("target", "shown", null);
            FixtureRuntime.StartCompanions(this);
        }

        protected override void OnActivated(EventArgs eventArgs)
        {
            bool becameGlobalForeground = FixtureRuntime.IsGlobalForeground(Handle.ToInt64());
            base.OnActivated(eventArgs);
            if (becameGlobalForeground)
            {
                activatedCount++;
                Dictionary<string, object> details = new Dictionary<string, object>();
                details["activatedCount"] = activatedCount;
                store.AppendEvent("target", "activated", details);
            }
        }

        private static Label NewLabel(string text, int x, int y, int width)
        {
            Label label = new Label();
            label.Text = text;
            label.Location = new Point(x, y);
            label.Size = new Size(width, 25);
            label.TextAlign = ContentAlignment.MiddleLeft;
            return label;
        }

        private IDictionary<string, object> Rect(Rectangle rectangle)
        {
            Dictionary<string, object> output = new Dictionary<string, object>();
            output["x"] = rectangle.X;
            output["y"] = rectangle.Y;
            output["width"] = rectangle.Width;
            output["height"] = rectangle.Height;
            return output;
        }

        private void WriteReady()
        {
            Dictionary<string, object> ready = new Dictionary<string, object>();
            ready["schemaVersion"] = 1;
            ready["processId"] = System.Diagnostics.Process.GetCurrentProcess().Id;
            ready["targetHwnd"] = Handle.ToInt64().ToString(CultureInfo.InvariantCulture);
            ready["surfaceHwnd"] = surface.Handle.ToInt64().ToString(CultureInfo.InvariantCulture);
            ready["sentinelHwnd"] = FixtureRuntime.SentinelHandle.ToString(CultureInfo.InvariantCulture);
            ready["armButtonHwnd"] = FixtureRuntime.ArmButtonHandle.ToString(CultureInfo.InvariantCulture);
            ready["occluderHwnd"] = FixtureRuntime.OccluderHandle.ToString(CultureInfo.InvariantCulture);
            ready["backdropHwnd"] = FixtureRuntime.BackdropHandle.ToString(CultureInfo.InvariantCulture);
            ready["occluderEnabled"] = FixtureRuntime.ShowOccluder;
            store.WriteReady(ready);
            store.AppendEvent("fixture", "ready", null);
            WriteState();
        }

        private void WriteState()
        {
            if (IsDisposed || !IsHandleCreated)
            {
                return;
            }
            Rectangle surfaceScreen = surface.RectangleToScreen(surface.ClientRectangle);
            Dictionary<string, object> state = new Dictionary<string, object>();
            state["schemaVersion"] = 1;
            state["statePublicationGeneration"] = Interlocked.Increment(ref statePublicationGeneration);
            state["utc"] = DateTime.UtcNow.ToString("o", CultureInfo.InvariantCulture);
            state["processId"] = System.Diagnostics.Process.GetCurrentProcess().Id;
            state["uptimeMs"] = (long)(DateTime.UtcNow - startedAt).TotalMilliseconds;
            state["ready"] = readyWritten;
            state["targetHwnd"] = Handle.ToInt64().ToString(CultureInfo.InvariantCulture);
            state["surfaceHwnd"] = surface.Handle.ToInt64().ToString(CultureInfo.InvariantCulture);
            state["sentinelHwnd"] = FixtureRuntime.SentinelHandle.ToString(CultureInfo.InvariantCulture);
            state["armButtonHwnd"] = FixtureRuntime.ArmButtonHandle.ToString(CultureInfo.InvariantCulture);
            state["occluderHwnd"] = FixtureRuntime.OccluderHandle.ToString(CultureInfo.InvariantCulture);
            state["backdropHwnd"] = FixtureRuntime.BackdropHandle.ToString(CultureInfo.InvariantCulture);
            state["targetBounds"] = Rect(Bounds);
            state["surfaceScreenBounds"] = Rect(surfaceScreen);
            state["animationFrame"] = animationFrame;
            state["invokeCount"] = invokeCount;
            state["semanticValue"] = new Dictionary<string, object>
            {
                { "length", semanticValue.TextLength },
                { "sha256", EvidenceStore.HashText(semanticValue.Text) }
            };
            state["focusedText"] = new Dictionary<string, object>
            {
                { "length", focusedText.TextLength },
                { "sha256", EvidenceStore.HashText(focusedText.Text) }
            };
            state["messageCounters"] = counters.Snapshot();
            state["targetActivatedCount"] = activatedCount;
            state["sentinelActivatedCount"] = FixtureRuntime.SentinelActivatedCount;
            state["sentinelDeactivatedCount"] = FixtureRuntime.SentinelDeactivatedCount;
            state["foregroundArmRequestedGeneration"] = FixtureRuntime.ForegroundArmRequestedGeneration;
            state["foregroundArmAcknowledgedGeneration"] = FixtureRuntime.ForegroundArmAcknowledgedGeneration;
            state["foregroundArmRequestCount"] = FixtureRuntime.ForegroundArmRequestCount;
            state["foregroundArmAcknowledgementCount"] = FixtureRuntime.ForegroundArmAcknowledgementCount;
            state["foregroundArmLeftMouseDownCount"] = FixtureRuntime.ForegroundArmLeftMouseDownCount;
            state["foregroundArmLeftMouseUpCount"] = FixtureRuntime.ForegroundArmLeftMouseUpCount;
            state["foregroundArmButtonEnabled"] = FixtureRuntime.ForegroundArmButtonEnabled;
            state["eventSequence"] = store.CurrentSequence;
            state["occluderEnabled"] = FixtureRuntime.ShowOccluder;
            store.WriteState(state);
        }
    }

    internal sealed class SentinelForm : Form
    {
        internal const string StableWindowTitle = "LBB Foreground Sentinel";
        private readonly EvidenceStore store;
        private readonly Label statusLabel;
        private readonly Button armButton;

        protected override bool ShowWithoutActivation
        {
            get { return true; }
        }

        protected override CreateParams CreateParams
        {
            get
            {
                const int WS_EX_NOACTIVATE = 0x08000000;
                CreateParams parameters = base.CreateParams;
                parameters.ExStyle |= WS_EX_NOACTIVATE;
                return parameters;
            }
        }

        internal SentinelForm(EvidenceStore evidenceStore)
        {
            store = evidenceStore;
            Text = StableWindowTitle;
            AccessibleName = StableWindowTitle;
            StartPosition = FormStartPosition.Manual;
            Rectangle area = Screen.PrimaryScreen.WorkingArea;
            Location = new Point(Math.Max(area.Left + 10, area.Right - 390), Math.Max(area.Top + 10, area.Bottom - 230));
            ClientSize = new Size(360, 170);
            TopMost = true;
            BackColor = Color.FromArgb(255, 183, 77);
            Font = new Font("Segoe UI", 10.0f, FontStyle.Bold, GraphicsUnit.Point);
            statusLabel = new Label();
            statusLabel.Location = new Point(0, 0);
            statusLabel.Size = new Size(360, 62);
            statusLabel.Anchor = AnchorStyles.Top | AnchorStyles.Left | AnchorStyles.Right;
            statusLabel.TextAlign = ContentAlignment.MiddleCenter;
            statusLabel.Text = "AUTOMATIC BASELINE\r\nno operator action required";
            Controls.Add(statusLabel);

            armButton = new Button();
            armButton.Name = "ForegroundArmButton";
            armButton.AccessibleName = "Automatic Windows acceptance baseline status";
            armButton.AccessibleDescription = "Disabled status surface; no click or operator action is required";
            armButton.Location = new Point(12, 68);
            armButton.Size = new Size(336, 90);
            armButton.Anchor = AnchorStyles.Top | AnchorStyles.Bottom | AnchorStyles.Left | AnchorStyles.Right;
            armButton.Enabled = false;
            armButton.Text = "NO ACTION REQUIRED";
            armButton.Font = new Font("Segoe UI", 15.0f, FontStyle.Bold, GraphicsUnit.Point);
            armButton.BackColor = Color.White;
            Controls.Add(armButton);
            Shown += delegate
            {
                FixtureRuntime.SentinelHandle = Handle.ToInt64();
                FixtureRuntime.ArmButtonHandle = armButton.Handle.ToInt64();
                store.AppendEvent("sentinel", "shown", null);
            };
        }

        protected override void OnActivated(EventArgs eventArgs)
        {
            bool becameGlobalForeground = FixtureRuntime.IsGlobalForeground(Handle.ToInt64());
            base.OnActivated(eventArgs);
            if (becameGlobalForeground)
            {
                FixtureRuntime.RecordSentinelForegroundActivation();
                store.AppendEvent("sentinel", "activated", null);
            }
        }

        protected override void OnDeactivate(EventArgs eventArgs)
        {
            base.OnDeactivate(eventArgs);
            if (FixtureRuntime.RecordSentinelForegroundDeactivation())
            {
                store.AppendEvent("sentinel", "deactivated", null);
            }
        }

        protected override void WndProc(ref Message message)
        {
            const int WM_LBUTTONDOWN = 0x0201;
            const int WM_LBUTTONUP = 0x0202;
            const int WM_NCLBUTTONDOWN = 0x00A1;
            const int WM_NCLBUTTONUP = 0x00A2;
            const int WM_PARENTNOTIFY = 0x0210;
            int observedMessage = message.Msg;
            if (message.Msg == WM_PARENTNOTIFY)
            {
                observedMessage = unchecked((int)((long)message.WParam & 0xffff));
            }
            bool leftDown = observedMessage == WM_LBUTTONDOWN || observedMessage == WM_NCLBUTTONDOWN;
            bool leftUp = observedMessage == WM_LBUTTONUP || observedMessage == WM_NCLBUTTONUP;
            if (leftDown || leftUp)
            {
                int count = leftDown
                    ? FixtureRuntime.RecordPassiveLeftMouseDown()
                    : FixtureRuntime.RecordPassiveLeftMouseUp();
                if (store != null)
                {
                    Dictionary<string, object> details = new Dictionary<string, object>();
                    details["count"] = count;
                    store.AppendEvent(
                        "sentinel",
                        leftDown ? "passiveLeftMouseDownObserved" : "passiveLeftMouseUpObserved",
                        details);
                }
            }
            base.WndProc(ref message);
        }
    }

    internal sealed class OccluderForm : Form
    {
        protected override bool ShowWithoutActivation
        {
            get { return true; }
        }

        protected override CreateParams CreateParams
        {
            get
            {
                const int WS_EX_NOACTIVATE = 0x08000000;
                CreateParams parameters = base.CreateParams;
                parameters.ExStyle |= WS_EX_NOACTIVATE;
                return parameters;
            }
        }

        internal OccluderForm(Rectangle targetBounds)
        {
            Text = "LBB Magenta Occluder";
            AccessibleName = "LBB Magenta Occluder";
            FormBorderStyle = FormBorderStyle.FixedToolWindow;
            ShowInTaskbar = false;
            StartPosition = FormStartPosition.Manual;
            Location = new Point(targetBounds.Right - 320, targetBounds.Top + 70);
            ClientSize = new Size(280, 180);
            TopMost = true;
            BackColor = Color.Magenta;
            Label label = new Label();
            label.Dock = DockStyle.Fill;
            label.ForeColor = Color.White;
            label.BackColor = Color.Magenta;
            label.Font = new Font("Segoe UI", 14.0f, FontStyle.Bold, GraphicsUnit.Point);
            label.TextAlign = ContentAlignment.MiddleCenter;
            label.Text = "MAGENTA OCCLUDER\r\nthis must not appear in exact-window capture";
            Controls.Add(label);
            Shown += delegate { FixtureRuntime.OccluderHandle = Handle.ToInt64(); };
        }
    }

    public static class FixtureRuntime
    {
        private static readonly ManualResetEvent sentinelReady = new ManualResetEvent(false);
        private static readonly ManualResetEvent occluderReady = new ManualResetEvent(false);
        private static SentinelForm sentinel;
        private static OccluderForm occluder;
        private static EvidenceStore store;
        private static int companionStarted;
        private static int sentinelActivated;
        private static int sentinelDeactivated;
        private static int sentinelForegroundActive;
        private static int passiveLeftMouseDown;
        private static int passiveLeftMouseUp;
        internal static bool ShowOccluder;
        internal static long SentinelHandle;
        internal static long ArmButtonHandle;
        internal static long OccluderHandle;
        internal static long BackdropHandle;

        internal static bool CompanionsReady
        {
            get
            {
                return sentinelReady.WaitOne(0) && (!ShowOccluder || occluderReady.WaitOne(0));
            }
        }

        internal static int SentinelActivatedCount
        {
            get { return Interlocked.CompareExchange(ref sentinelActivated, 0, 0); }
        }

        internal static int SentinelDeactivatedCount
        {
            get { return Interlocked.CompareExchange(ref sentinelDeactivated, 0, 0); }
        }

        internal static int ForegroundArmRequestedGeneration
        {
            get { return 0; }
        }

        internal static int ForegroundArmAcknowledgedGeneration
        {
            get { return 0; }
        }

        internal static int ForegroundArmRequestCount
        {
            get { return 0; }
        }

        internal static int ForegroundArmAcknowledgementCount
        {
            get { return 0; }
        }

        internal static int ForegroundArmLeftMouseDownCount
        {
            get { return Interlocked.CompareExchange(ref passiveLeftMouseDown, 0, 0); }
        }

        internal static int ForegroundArmLeftMouseUpCount
        {
            get { return Interlocked.CompareExchange(ref passiveLeftMouseUp, 0, 0); }
        }

        internal static bool ForegroundArmButtonEnabled
        {
            get { return false; }
        }

        [DllImport("user32.dll")]
        private static extern IntPtr GetForegroundWindow();

        internal static bool IsGlobalForeground(long windowHandle)
        {
            return windowHandle != 0 && GetForegroundWindow().ToInt64() == windowHandle;
        }

        public static void RunSelfTest()
        {
            Dictionary<string, object> nestedJson = new Dictionary<string, object>();
            nestedJson["quote"] = "a\"b";
            nestedJson["ok"] = true;
            Dictionary<string, object> jsonFixture = new Dictionary<string, object>();
            jsonFixture["schemaVersion"] = 1;
            jsonFixture["nested"] = nestedJson;
            if (JsonValueWriter.Serialize(jsonFixture) != "{\"schemaVersion\":1,\"nested\":{\"quote\":\"a\\\"b\",\"ok\":true}}")
            {
                throw new InvalidOperationException("The cross-runtime fixture JSON writer failed its self-test.");
            }
            jsonFixture["nonFinite"] = Double.NaN;
            bool nonFiniteRefused = false;
            try
            {
                JsonValueWriter.Serialize(jsonFixture);
            }
            catch (InvalidOperationException)
            {
                nonFiniteRefused = true;
            }
            if (!nonFiniteRefused)
            {
                throw new InvalidOperationException("The fixture JSON writer accepted a non-finite number.");
            }
            if (SentinelForm.StableWindowTitle != "LBB Foreground Sentinel")
            {
                throw new InvalidOperationException("The stable foreground-sentinel window title failed its self-test.");
            }
            if (ForegroundArmRequestedGeneration != 0 ||
                ForegroundArmAcknowledgedGeneration != 0 ||
                ForegroundArmRequestCount != 0 ||
                ForegroundArmAcknowledgementCount != 0 ||
                ForegroundArmLeftMouseDownCount != 0 ||
                ForegroundArmLeftMouseUpCount != 0 ||
                ForegroundArmButtonEnabled)
            {
                throw new InvalidOperationException("The automatic baseline must expose zero arm and input-attempt state.");
            }
            if (RecordPassiveLeftMouseDown() != 1 ||
                RecordPassiveLeftMouseUp() != 1 ||
                ForegroundArmLeftMouseDownCount != 1 ||
                ForegroundArmLeftMouseUpCount != 1)
            {
                throw new InvalidOperationException("Passive non-authorizing sentinel input-attempt instrumentation failed its self-test.");
            }
        }

        internal static void RecordSentinelForegroundActivation()
        {
            if (Interlocked.Exchange(ref sentinelForegroundActive, 1) == 0)
            {
                Interlocked.Increment(ref sentinelActivated);
            }
        }

        internal static bool RecordSentinelForegroundDeactivation()
        {
            if (Interlocked.Exchange(ref sentinelForegroundActive, 0) == 0)
            {
                return false;
            }
            Interlocked.Increment(ref sentinelDeactivated);
            return true;
        }

        internal static int RecordPassiveLeftMouseDown()
        {
            return Interlocked.Increment(ref passiveLeftMouseDown);
        }

        internal static int RecordPassiveLeftMouseUp()
        {
            return Interlocked.Increment(ref passiveLeftMouseUp);
        }

        public static void Run(string evidenceDirectory, bool showOccluder)
        {
            store = new EvidenceStore(evidenceDirectory);
            ShowOccluder = showOccluder;
            Exception failure = null;
            Thread targetThread = new Thread(new ThreadStart(delegate
            {
                try
                {
                    Application.EnableVisualStyles();
                    Application.SetCompatibleTextRenderingDefault(false);
                    MessageCounters counters = new MessageCounters();
                    using (TargetForm target = new TargetForm(store, counters))
                    using (BackdropForm backdrop = new BackdropForm(target.Bounds))
                    {
                        backdrop.Show();
                        BackdropHandle = backdrop.Handle.ToInt64();
                        store.AppendEvent("backdrop", "shown", null);
                        target.FormClosed += delegate { Application.ExitThread(); };
                        backdrop.Bounds = Rectangle.Inflate(target.Bounds, 28, 28);
                        target.ShowNonActivating();
                        Application.Run();
                        backdrop.Close();
                    }
                }
                catch (Exception error)
                {
                    failure = error;
                    Dictionary<string, object> details = new Dictionary<string, object>();
                    details["type"] = error.GetType().FullName;
                    details["message"] = "The fixture target UI failed before normal shutdown.";
                    store.AppendEvent("fixture", "fatalError", details);
                }
            }));
            targetThread.Name = "LBB fixture target UI";
            targetThread.SetApartmentState(ApartmentState.STA);
            targetThread.Start();
            targetThread.Join();
            if (failure != null)
            {
                throw new InvalidOperationException("The fixture target UI failed: " + failure.Message, failure);
            }
        }

        internal static void StartCompanions(TargetForm target)
        {
            if (Interlocked.Exchange(ref companionStarted, 1) != 0)
            {
                return;
            }

            if (ShowOccluder)
            {
                Rectangle targetBounds = target.Bounds;
                Thread occluderThread = new Thread(new ThreadStart(delegate
                {
                    occluder = new OccluderForm(targetBounds);
                    occluder.Shown += delegate
                    {
                        store.AppendEvent("occluder", "shown", null);
                        occluderReady.Set();
                    };
                    Application.Run(occluder);
                }));
                occluderThread.Name = "LBB fixture occluder UI";
                occluderThread.IsBackground = true;
                occluderThread.SetApartmentState(ApartmentState.STA);
                occluderThread.Start();
            }
            else
            {
                occluderReady.Set();
            }

            Thread sentinelThread = new Thread(new ThreadStart(delegate
            {
                sentinel = new SentinelForm(store);
                sentinel.Shown += delegate { sentinelReady.Set(); };
                Application.Run(sentinel);
            }));
            sentinelThread.Name = "LBB fixture sentinel UI";
            sentinelThread.IsBackground = true;
            sentinelThread.SetApartmentState(ApartmentState.STA);
            sentinelThread.Start();
        }

        internal static void StopCompanions()
        {
            CloseForm(sentinel);
            CloseForm(occluder);
        }

        private static void CloseForm(Form form)
        {
            if (form == null || form.IsDisposed || !form.IsHandleCreated)
            {
                return;
            }
            try
            {
                form.BeginInvoke(new MethodInvoker(form.Close));
            }
            catch (InvalidOperationException)
            {
            }
        }
    }
}
'@

$fixtureNamespace = "LbbWindowsFixture_" + [Guid]::NewGuid().ToString("N")
$fixtureSource = $fixtureSource.Replace("namespace LbbWindowsFixture", "namespace $fixtureNamespace")
$fixtureProgramSource = @'
namespace LbbWindowsFixture
{
    using System;
    using System.IO;
    using System.Runtime.InteropServices;

    internal static class FixtureProgram
    {
        private const string AppUserModelId = "LocalBrowserBridge.WindowsAcceptance";

        [DllImport("shell32.dll", CharSet = CharSet.Unicode)]
        private static extern int SetCurrentProcessExplicitAppUserModelID(string appId);

        private static bool TryParseArguments(
            string[] args,
            out bool selfTest,
            out string evidenceDirectory,
            out bool showOccluder)
        {
            selfTest = args != null && args.Length == 1 &&
                String.Equals(args[0], "--self-test", StringComparison.Ordinal);
            evidenceDirectory = null;
            showOccluder = false;
            if (selfTest)
            {
                return true;
            }
            if (args == null || (args.Length != 2 && args.Length != 3) ||
                !String.Equals(args[0], "--evidence-directory", StringComparison.Ordinal) ||
                String.IsNullOrWhiteSpace(args[1]) ||
                !IsDriveAbsoluteNonRootPath(args[1]) ||
                (args.Length == 3 &&
                    !String.Equals(args[2], "--show-occluder", StringComparison.Ordinal)))
            {
                return false;
            }
            evidenceDirectory = Path.GetFullPath(args[1]);
            showOccluder = args.Length == 3;
            return true;
        }

        private static bool IsDriveAbsoluteNonRootPath(string value)
        {
            if (String.IsNullOrWhiteSpace(value) || value.Length <= 3 ||
                !Char.IsLetter(value[0]) || value[1] != ':' ||
                (value[2] != Path.DirectorySeparatorChar && value[2] != Path.AltDirectorySeparatorChar))
            {
                return false;
            }
            string fullPath = Path.GetFullPath(value);
            string root = Path.GetPathRoot(fullPath);
            return !String.IsNullOrEmpty(root) &&
                !String.Equals(fullPath.TrimEnd(Path.DirectorySeparatorChar, Path.AltDirectorySeparatorChar),
                    root.TrimEnd(Path.DirectorySeparatorChar, Path.AltDirectorySeparatorChar),
                    StringComparison.OrdinalIgnoreCase);
        }

        private static void RunArgumentSelfTest()
        {
            string root = Path.GetPathRoot(Environment.SystemDirectory);
            string absoluteEvidence = Path.Combine(root, "lbb-fixture-self-test-evidence");
            bool selfTest;
            string evidenceDirectory;
            bool showOccluder;
            if (TryParseArguments(null, out selfTest, out evidenceDirectory, out showOccluder) ||
                TryParseArguments(new string[0], out selfTest, out evidenceDirectory, out showOccluder) ||
                TryParseArguments(new string[] { "--unknown" }, out selfTest, out evidenceDirectory, out showOccluder) ||
                TryParseArguments(new string[] { "--self-test", "extra" }, out selfTest, out evidenceDirectory, out showOccluder) ||
                TryParseArguments(new string[] { "--SELF-TEST" }, out selfTest, out evidenceDirectory, out showOccluder) ||
                TryParseArguments(new string[] { "--evidence-directory" }, out selfTest, out evidenceDirectory, out showOccluder) ||
                TryParseArguments(new string[] { "--evidence-directory", "relative" }, out selfTest, out evidenceDirectory, out showOccluder) ||
                TryParseArguments(new string[] { "--evidence-directory", "C:drive-relative" }, out selfTest, out evidenceDirectory, out showOccluder) ||
                TryParseArguments(new string[] { "--evidence-directory", "C:\\" }, out selfTest, out evidenceDirectory, out showOccluder) ||
                TryParseArguments(new string[] { "--evidence-directory", "\\root-relative" }, out selfTest, out evidenceDirectory, out showOccluder) ||
                TryParseArguments(new string[] { "--evidence-directory", "\\\\server\\share\\evidence" }, out selfTest, out evidenceDirectory, out showOccluder) ||
                TryParseArguments(new string[] { absoluteEvidence, "--evidence-directory" }, out selfTest, out evidenceDirectory, out showOccluder) ||
                TryParseArguments(new string[] { "--evidence-directory", absoluteEvidence, "--show-occluder", "--show-occluder" }, out selfTest, out evidenceDirectory, out showOccluder) ||
                TryParseArguments(new string[] { "--evidence-directory", absoluteEvidence, "--SHOW-OCCLUDER" }, out selfTest, out evidenceDirectory, out showOccluder))
            {
                throw new InvalidOperationException("The dedicated fixture accepted an invalid argument sequence.");
            }
            if (!TryParseArguments(new string[] { "--self-test" }, out selfTest, out evidenceDirectory, out showOccluder) ||
                !selfTest || evidenceDirectory != null || showOccluder ||
                !TryParseArguments(new string[] { "--evidence-directory", absoluteEvidence }, out selfTest, out evidenceDirectory, out showOccluder) ||
                selfTest || evidenceDirectory != absoluteEvidence || showOccluder ||
                !TryParseArguments(new string[] { "--evidence-directory", absoluteEvidence, "--show-occluder" }, out selfTest, out evidenceDirectory, out showOccluder) ||
                selfTest || evidenceDirectory != absoluteEvidence || !showOccluder)
            {
                throw new InvalidOperationException("The dedicated fixture rejected a canonical argument sequence.");
            }

            string freshnessRoot = Path.Combine(
                Path.GetTempPath(),
                "lbb-fixture-entrypoint-self-test-" + Guid.NewGuid().ToString("N"));
            string protectedFile = Path.Combine(freshnessRoot, "fixture-state.json");
            string protectedDirectory = Path.Combine(freshnessRoot, "fixture-ready.json");
            try
            {
                Directory.CreateDirectory(freshnessRoot);
                if (!IsFreshEvidenceDirectory(freshnessRoot))
                {
                    throw new InvalidOperationException("The dedicated fixture rejected a fresh evidence directory.");
                }
                File.WriteAllBytes(protectedFile, new byte[] { 1 });
                if (IsFreshEvidenceDirectory(freshnessRoot))
                {
                    throw new InvalidOperationException("The dedicated fixture accepted a protected evidence file.");
                }
                File.Delete(protectedFile);
                Directory.CreateDirectory(protectedDirectory);
                if (IsFreshEvidenceDirectory(freshnessRoot))
                {
                    throw new InvalidOperationException("The dedicated fixture accepted a protected evidence directory.");
                }
                Directory.Delete(protectedDirectory, false);
            }
            finally
            {
                if (File.Exists(protectedFile))
                {
                    File.Delete(protectedFile);
                }
                if (Directory.Exists(protectedDirectory))
                {
                    Directory.Delete(protectedDirectory, false);
                }
                if (Directory.Exists(freshnessRoot))
                {
                    Directory.Delete(freshnessRoot, false);
                }
            }
        }

        private static bool PathEntryExists(string path)
        {
            try
            {
                File.GetAttributes(path);
                return true;
            }
            catch (FileNotFoundException)
            {
                return false;
            }
            catch (DirectoryNotFoundException)
            {
                return false;
            }
        }

        private static bool IsFreshEvidenceDirectory(string evidenceDirectory)
        {
            if (!Directory.Exists(evidenceDirectory))
            {
                return false;
            }
            FileAttributes attributes = File.GetAttributes(evidenceDirectory);
            if ((attributes & FileAttributes.Directory) == 0 ||
                (attributes & FileAttributes.ReparsePoint) != 0)
            {
                return false;
            }
            string[] protectedNames = new string[]
            {
                "fixture-state.json",
                "fixture-events.ndjson",
                "fixture-ready.json"
            };
            foreach (string protectedName in protectedNames)
            {
                if (PathEntryExists(Path.Combine(evidenceDirectory, protectedName)))
                {
                    return false;
                }
            }
            return true;
        }

        [STAThread]
        private static int Main(string[] args)
        {
            try
            {
                bool selfTest;
                string evidenceDirectory;
                bool showOccluder;
                if (!TryParseArguments(args, out selfTest, out evidenceDirectory, out showOccluder))
                {
                    return 2;
                }
                if (!selfTest && !IsFreshEvidenceDirectory(evidenceDirectory))
                {
                    return 2;
                }
                int appIdResult = SetCurrentProcessExplicitAppUserModelID(AppUserModelId);
                if (appIdResult != 0)
                {
                    return 3;
                }
                if (selfTest)
                {
                    RunArgumentSelfTest();
                    FixtureRuntime.RunSelfTest();
                }
                else
                {
                    FixtureRuntime.Run(evidenceDirectory, showOccluder);
                }
                return 0;
            }
            catch
            {
                return 1;
            }
        }
    }
}
'@
$fixtureProgramSource = $fixtureProgramSource.Replace(
    "namespace LbbWindowsFixture",
    "namespace $fixtureNamespace"
)

if ($PSCmdlet.ParameterSetName -eq "Build") {
    $sourceSha256BeforeBuild = Get-FixtureSourceSha256 $PSCommandPath
    if ($sourceSha256BeforeBuild -cne $ExpectedSourceSha256) {
        throw "ExpectedSourceSha256 does not match the exact fixture source."
    }
    $outputPath = [IO.Path]::GetFullPath($BuildExecutablePath)
    if ([IO.Path]::GetExtension($outputPath) -cne ".exe") {
        throw "BuildExecutablePath must name a new .exe file."
    }
    if ([IO.File]::Exists($outputPath) -or [IO.Directory]::Exists($outputPath)) {
        throw "BuildExecutablePath must not already exist."
    }
    $outputDirectory = [IO.Path]::GetDirectoryName($outputPath)
    if ([String]::IsNullOrWhiteSpace($outputDirectory) -or
        -not [IO.Directory]::Exists($outputDirectory)) {
        throw "BuildExecutablePath must have an existing parent directory."
    }
    Add-Type -TypeDefinition ($fixtureSource + [Environment]::NewLine + $fixtureProgramSource) `
        -Language CSharp `
        -OutputAssembly $outputPath `
        -OutputType WindowsApplication `
        -ReferencedAssemblies @(
            "System.Windows.Forms",
            "System.Drawing"
        )
    $sourceSha256AfterBuild = Get-FixtureSourceSha256 $PSCommandPath
    if ($sourceSha256AfterBuild -cne $ExpectedSourceSha256) {
        if ([IO.File]::Exists($outputPath)) {
            [IO.File]::Delete($outputPath)
        }
        throw "The fixture source changed during the dedicated executable build."
    }
    if (-not [IO.File]::Exists($outputPath) -or ([IO.FileInfo]::new($outputPath)).Length -le 0) {
        throw "The dedicated Windows fixture executable was not produced."
    }
    Write-Output "Windows computer-use fixture executable built."
    return
}

$fixtureSelfTestReferences = @(
    "System.Windows.Forms",
    "System.Drawing"
)
if ($PSVersionTable.PSEdition -ceq "Core") {
    $fixtureSelfTestReferences += @(
        "System.Collections",
        "System.Collections.Specialized",
        "System.ComponentModel.Primitives",
        "System.ComponentModel.TypeConverter",
        "System.Diagnostics.Process",
        "System.Drawing.Common",
        "System.Drawing.Primitives",
        "System.Private.Windows.Core",
        "System.Private.Windows.GdiPlus",
        "System.Runtime",
        "System.Runtime.Extensions",
        "System.Runtime.InteropServices",
        "System.Security.Cryptography",
        "System.Security.Cryptography.Algorithms",
        "System.Security.Cryptography.Primitives",
        "System.Text.Encoding",
        "System.Text.Encoding.Extensions",
        "System.Threading",
        "System.Threading.Thread",
        "System.Windows.Forms.Primitives"
    )
}
Add-Type -TypeDefinition $fixtureSource -Language CSharp -ReferencedAssemblies $fixtureSelfTestReferences
$fixtureRuntimeType = ("$fixtureNamespace.FixtureRuntime" -as [type])
if ($null -eq $fixtureRuntimeType) {
    throw "The isolated Windows fixture runtime type did not load."
}

if ($SelfTest) {
    $targetSourceStart = $fixtureSource.IndexOf(
        'internal sealed class TargetForm : Form',
        [StringComparison]::Ordinal
    )
    $sentinelSourceStart = $fixtureSource.IndexOf(
        'internal sealed class SentinelForm : Form',
        [StringComparison]::Ordinal
    )
    $sentinelSourceEnd = $fixtureSource.IndexOf(
        'internal sealed class OccluderForm : Form',
        $sentinelSourceStart + 1,
        [StringComparison]::Ordinal
    )
    if ($targetSourceStart -lt 0 -or
        $sentinelSourceStart -le $targetSourceStart -or
        $sentinelSourceEnd -le $sentinelSourceStart) {
        throw "The target/sentinel source boundaries failed their self-test."
    }
    $targetSource = $fixtureSource.Substring(
        $targetSourceStart,
        $sentinelSourceStart - $targetSourceStart
    )
    $sentinelSource = $fixtureSource.Substring(
        $sentinelSourceStart,
        $sentinelSourceEnd - $sentinelSourceStart
    )
    $sentinelTitleAssignments = [regex]::Matches(
        $sentinelSource,
        '(?m)^[ \t]*Text[ \t]*=[ \t]*StableWindowTitle;[ \t]*$'
    )
    $allTopLevelTitleAssignments = [regex]::Matches(
        $sentinelSource,
        '(?m)^[ \t]*Text[ \t]*='
    )
    if ($sentinelTitleAssignments.Count -ne 1 -or $allTopLevelTitleAssignments.Count -ne 1) {
        throw "The foreground-sentinel top-level title must be assigned exactly once from its stable creation-time constant."
    }
    if ($sentinelSource.IndexOf(('LBB Windows Acceptance - ACTION' + ' REQUIRED'), [StringComparison]::Ordinal) -ge 0 -or
        $sentinelSource.IndexOf(('LBB Windows Acceptance - ' + 'ARMED'), [StringComparison]::Ordinal) -ge 0) {
        throw "The foreground-sentinel top-level title must not encode its arm state."
    }
    foreach ($nonactivatingSource in @($targetSource, $sentinelSource)) {
        if ($nonactivatingSource.IndexOf('protected override bool ShowWithoutActivation', [StringComparison]::Ordinal) -lt 0 -or
            $nonactivatingSource.IndexOf('parameters.ExStyle |= WS_EX_NOACTIVATE;', [StringComparison]::Ordinal) -lt 0) {
            throw "Every target and sentinel top-level window must be explicitly nonactivating."
        }
    }
    if ([regex]::Matches($fixtureSource, [regex]::Escape('protected override bool ShowWithoutActivation')).Count -ne 4 -or
        [regex]::Matches($fixtureSource, [regex]::Escape('parameters.ExStyle |= WS_EX_NOACTIVATE')).Count -ne 4) {
        throw "All four fixture top-level windows must preserve their nonactivating contract."
    }
    if ($targetSource.IndexOf('ShowWindow(window, SW_SHOWNOACTIVATE);', [StringComparison]::Ordinal) -lt 0 -or
        $fixtureSource.IndexOf('target.ShowNonActivating();', [StringComparison]::Ordinal) -lt 0 -or
        $fixtureSource.IndexOf('Application.Run(target);', [StringComparison]::Ordinal) -ge 0) {
        throw "The target must enter its message loop through an explicit nonactivating Win32 show."
    }
    if ($targetSource.IndexOf('FixtureRuntime.IsGlobalForeground(Handle.ToInt64())', [StringComparison]::Ordinal) -lt 0 -or
        $sentinelSource.IndexOf('FixtureRuntime.IsGlobalForeground(Handle.ToInt64())', [StringComparison]::Ordinal) -lt 0 -or
        $sentinelSource.IndexOf('RecordSentinelForegroundDeactivation()', [StringComparison]::Ordinal) -lt 0) {
        throw "Activation counters must record only real OS-global foreground transitions."
    }
    if ($sentinelSource.IndexOf('statusLabel.Text = "AUTOMATIC BASELINE\r\nno operator action required";', [StringComparison]::Ordinal) -lt 0 -or
        $sentinelSource.IndexOf('armButton.Enabled = false;', [StringComparison]::Ordinal) -lt 0 -or
        $sentinelSource.IndexOf('armButton.Text = "NO ACTION REQUIRED";', [StringComparison]::Ordinal) -lt 0 -or
        $sentinelSource.IndexOf('protected override void WndProc(ref Message message)', [StringComparison]::Ordinal) -lt 0 -or
        $sentinelSource.IndexOf('WM_PARENTNOTIFY', [StringComparison]::Ordinal) -lt 0 -or
        $sentinelSource.IndexOf('RecordPassiveLeftMouseDown()', [StringComparison]::Ordinal) -lt 0 -or
        $sentinelSource.IndexOf('RecordPassiveLeftMouseUp()', [StringComparison]::Ordinal) -lt 0) {
        throw "The foreground status surface did not preserve its disabled automatic-baseline contract."
    }
    foreach ($forbiddenActivationSource in @(
        ('BringTo' + 'Front();'),
        ('.' + 'Focus();'),
        ('.' + 'Select();'),
        ('MouseDown ' + '+='),
        ('MouseUp ' + '+='),
        ('ForegroundArm' + 'Message'),
        ('TryAcknowledge' + 'ForegroundArm'),
        ('Record' + 'ForegroundArm'),
        ('SetForeground' + 'Window'),
        ('Send' + 'Input(')
    )) {
        if ($fixtureSource.IndexOf($forbiddenActivationSource, [StringComparison]::Ordinal) -ge 0) {
            throw "The fixture retained a forbidden activation or synthetic-input path."
        }
    }
    if ($fixtureProgramSource.IndexOf(
            'private const string AppUserModelId = "LocalBrowserBridge.WindowsAcceptance";',
            [StringComparison]::Ordinal
        ) -lt 0 -or
        $fixtureProgramSource.IndexOf(
            'SetCurrentProcessExplicitAppUserModelID(AppUserModelId)',
            [StringComparison]::Ordinal
        ) -lt 0 -or
        $fixtureProgramSource.IndexOf(
            'String.Equals(args[0], "--evidence-directory", StringComparison.Ordinal)',
            [StringComparison]::Ordinal
        ) -lt 0 -or
        $fixtureProgramSource.IndexOf(
            '!IsDriveAbsoluteNonRootPath(args[1])',
            [StringComparison]::Ordinal
        ) -lt 0 -or
        $fixtureProgramSource.IndexOf(
            'String.Equals(args[0], "--self-test", StringComparison.Ordinal)',
            [StringComparison]::Ordinal
        ) -lt 0 -or
        $fixtureProgramSource.IndexOf(
            'String.Equals(args[2], "--show-occluder", StringComparison.Ordinal)',
            [StringComparison]::Ordinal
        ) -lt 0 -or
        $fixtureProgramSource.IndexOf(
            'if (PathEntryExists(Path.Combine(evidenceDirectory, protectedName)))',
            [StringComparison]::Ordinal
        ) -lt 0) {
        throw "The dedicated fixture executable identity or exact argument grammar failed its self-test."
    }
    $fixtureRuntimeType::RunSelfTest()
    Write-Output "Windows computer-use fixture self-test passed."
    return
}

$fixtureRuntimeType::Run($evidenceRoot, $ShowOccluder.IsPresent)
