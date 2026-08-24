#requires -Version 5.1

[CmdletBinding(DefaultParameterSetName = "Run")]
param(
    [Parameter(ParameterSetName = "Run", Mandatory = $true)]
    [ValidateNotNullOrEmpty()]
    [string]$EvidenceDirectory,

    [Parameter(ParameterSetName = "Run")]
    [switch]$ShowOccluder,

    [Parameter(ParameterSetName = "SelfTest", Mandatory = $true)]
    [switch]$SelfTest
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) {
    throw "The Windows computer-use fixture can run only on Windows."
}

$evidenceRoot = $null
if (-not $SelfTest) {
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
using System.Web.Script.Serialization;

namespace LbbWindowsFixture
{
    internal static class NativeMethods
    {
        [StructLayout(LayoutKind.Sequential)]
        internal struct POINT
        {
            internal int X;
            internal int Y;
        }

        [DllImport("user32.dll")]
        internal static extern IntPtr GetForegroundWindow();

        [DllImport("user32.dll")]
        internal static extern IntPtr GetFocus();

        [DllImport("user32.dll")]
        [return: MarshalAs(UnmanagedType.Bool)]
        internal static extern bool GetCursorPos(out POINT point);
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
                JavaScriptSerializer serializer = new JavaScriptSerializer();
                File.AppendAllText(eventPath, serializer.Serialize(record) + Environment.NewLine, new UTF8Encoding(false));
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
                JavaScriptSerializer serializer = new JavaScriptSerializer();
                File.WriteAllText(statePath, serializer.Serialize(state), new UTF8Encoding(false));
            }
        }

        internal void WriteReady(IDictionary<string, object> ready)
        {
            lock (gate)
            {
                JavaScriptSerializer serializer = new JavaScriptSerializer();
                File.WriteAllText(readyPath, serializer.Serialize(ready), new UTF8Encoding(false));
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
            focusedText.AccessibleDescription = "Retains the target UI thread focus chain while the sentinel owns the foreground";
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

            Shown += delegate
            {
                focusedText.Select();
                focusedText.Focus();
                timer.Start();
                store.AppendEvent("target", "shown", null);
                FixtureRuntime.StartCompanions(this);
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

        protected override void OnActivated(EventArgs eventArgs)
        {
            activatedCount++;
            base.OnActivated(eventArgs);
            if (store != null)
            {
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
            NativeMethods.POINT cursor;
            bool hasCursor = NativeMethods.GetCursorPos(out cursor);
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
            state["foregroundHwnd"] = NativeMethods.GetForegroundWindow().ToInt64().ToString(CultureInfo.InvariantCulture);
            state["targetBounds"] = Rect(Bounds);
            state["surfaceScreenBounds"] = Rect(surfaceScreen);
            state["cursor"] = hasCursor
                ? (object)new Dictionary<string, object> { { "x", cursor.X }, { "y", cursor.Y } }
                : null;
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
        private int pressedArmGeneration;

        protected override bool ShowWithoutActivation
        {
            get { return true; }
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
            statusLabel.Text = "FOREGROUND SENTINEL\r\nwait for the arm request before clicking";
            Controls.Add(statusLabel);

            armButton = new Button();
            armButton.Name = "ForegroundArmButton";
            armButton.AccessibleName = "Click to arm Windows acceptance";
            armButton.AccessibleDescription = "Requires a fresh left-mouse click after the runner requests foreground arming";
            armButton.Location = new Point(12, 68);
            armButton.Size = new Size(336, 90);
            armButton.Anchor = AnchorStyles.Top | AnchorStyles.Bottom | AnchorStyles.Left | AnchorStyles.Right;
            armButton.Enabled = false;
            armButton.Text = "WAITING FOR RUNNER";
            armButton.Font = new Font("Segoe UI", 15.0f, FontStyle.Bold, GraphicsUnit.Point);
            armButton.BackColor = Color.White;
            armButton.MouseDown += delegate(object sender, MouseEventArgs eventArgs)
            {
                if (eventArgs.Button == MouseButtons.Left)
                {
                    FixtureRuntime.RecordForegroundArmLeftMouseDown();
                }
                if (eventArgs.Button != MouseButtons.Left ||
                    !armButton.ClientRectangle.Contains(eventArgs.Location) ||
                    NativeMethods.GetForegroundWindow() != Handle ||
                    NativeMethods.GetFocus() != armButton.Handle)
                {
                    pressedArmGeneration = 0;
                    return;
                }
                int requested = FixtureRuntime.ForegroundArmRequestedGeneration;
                if (requested > 0 && requested != FixtureRuntime.ForegroundArmAcknowledgedGeneration)
                {
                    pressedArmGeneration = requested;
                }
                else
                {
                    pressedArmGeneration = 0;
                }
            };
            armButton.LostFocus += delegate { pressedArmGeneration = 0; };
            armButton.MouseUp += delegate(object sender, MouseEventArgs eventArgs)
            {
                if (eventArgs.Button == MouseButtons.Left)
                {
                    FixtureRuntime.RecordForegroundArmLeftMouseUp();
                }
                int pressed = pressedArmGeneration;
                pressedArmGeneration = 0;
                if (eventArgs.Button != MouseButtons.Left ||
                    pressed <= 0 ||
                    pressed != FixtureRuntime.ForegroundArmRequestedGeneration ||
                    !armButton.ClientRectangle.Contains(eventArgs.Location) ||
                    NativeMethods.GetForegroundWindow() != Handle ||
                    NativeMethods.GetFocus() != armButton.Handle)
                {
                    return;
                }
                if (FixtureRuntime.TryAcknowledgeForegroundArm(pressed))
                {
                    statusLabel.Text = "ARMED\r\nDo not use this session until the run finishes";
                    armButton.Text = "ARMED - DO NOT USE THIS SESSION";
                    armButton.BackColor = Color.FromArgb(198, 239, 206);
                    Dictionary<string, object> details = new Dictionary<string, object>();
                    details["generation"] = pressed;
                    store.AppendEvent("sentinel", "foregroundArmAcknowledged", details);
                }
            };
            Controls.Add(armButton);
            Shown += delegate
            {
                FixtureRuntime.SentinelHandle = Handle.ToInt64();
                FixtureRuntime.ArmButtonHandle = armButton.Handle.ToInt64();
                store.AppendEvent("sentinel", "shown", null);
            };
        }

        protected override void WndProc(ref Message message)
        {
            if (message.Msg == FixtureRuntime.ForegroundArmMessage)
            {
                int generation = message.WParam.ToInt32();
                if (FixtureRuntime.RecordForegroundArmRequest(generation))
                {
                    pressedArmGeneration = 0;
                    statusLabel.Text = "ACTION REQUIRED\r\nClick once, then stop using this session";
                    armButton.Enabled = true;
                    FixtureRuntime.MarkForegroundArmButtonEnabled();
                    armButton.Text = "CLICK TO ARM";
                    Dictionary<string, object> details = new Dictionary<string, object>();
                    details["generation"] = generation;
                    store.AppendEvent("sentinel", "foregroundArmRequested", details);
                }
                return;
            }
            base.WndProc(ref message);
        }

        protected override void OnActivated(EventArgs eventArgs)
        {
            FixtureRuntime.IncrementSentinelActivated();
            base.OnActivated(eventArgs);
            if (store != null)
            {
                store.AppendEvent("sentinel", "activated", null);
            }
        }

        protected override void OnDeactivate(EventArgs eventArgs)
        {
            pressedArmGeneration = 0;
            FixtureRuntime.IncrementSentinelDeactivated();
            base.OnDeactivate(eventArgs);
            if (store != null)
            {
                store.AppendEvent("sentinel", "deactivated", null);
            }
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
        // Acceptance-only UI handshake; this is not a product command surface.
        internal const int ForegroundArmMessage = 0x8126;
        private static readonly ManualResetEvent sentinelReady = new ManualResetEvent(false);
        private static readonly ManualResetEvent occluderReady = new ManualResetEvent(false);
        private static SentinelForm sentinel;
        private static OccluderForm occluder;
        private static EvidenceStore store;
        private static int companionStarted;
        private static int sentinelActivated;
        private static int sentinelDeactivated;
        private static int foregroundArmRequestedGeneration;
        private static int foregroundArmAcknowledgedGeneration;
        private static int foregroundArmRequestCount;
        private static int foregroundArmAcknowledgementCount;
        private static int foregroundArmLeftMouseDownCount;
        private static int foregroundArmLeftMouseUpCount;
        private static int foregroundArmButtonEnabled;

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
            get { return Interlocked.CompareExchange(ref foregroundArmRequestedGeneration, 0, 0); }
        }

        internal static int ForegroundArmAcknowledgedGeneration
        {
            get { return Interlocked.CompareExchange(ref foregroundArmAcknowledgedGeneration, 0, 0); }
        }

        internal static int ForegroundArmRequestCount
        {
            get { return Interlocked.CompareExchange(ref foregroundArmRequestCount, 0, 0); }
        }

        internal static int ForegroundArmAcknowledgementCount
        {
            get { return Interlocked.CompareExchange(ref foregroundArmAcknowledgementCount, 0, 0); }
        }

        internal static int ForegroundArmLeftMouseDownCount
        {
            get { return Interlocked.CompareExchange(ref foregroundArmLeftMouseDownCount, 0, 0); }
        }

        internal static int ForegroundArmLeftMouseUpCount
        {
            get { return Interlocked.CompareExchange(ref foregroundArmLeftMouseUpCount, 0, 0); }
        }

        internal static bool ForegroundArmButtonEnabled
        {
            get { return Interlocked.CompareExchange(ref foregroundArmButtonEnabled, 0, 0) == 1; }
        }

        internal static void RecordForegroundArmLeftMouseDown()
        {
            Interlocked.Increment(ref foregroundArmLeftMouseDownCount);
        }

        internal static void RecordForegroundArmLeftMouseUp()
        {
            Interlocked.Increment(ref foregroundArmLeftMouseUpCount);
        }

        internal static void MarkForegroundArmButtonEnabled()
        {
            Interlocked.Exchange(ref foregroundArmButtonEnabled, 1);
        }

        internal static bool RecordForegroundArmRequest(int generation)
        {
            if (generation <= 0)
            {
                return false;
            }
            Interlocked.Increment(ref foregroundArmRequestCount);
            int previous = Interlocked.Exchange(ref foregroundArmRequestedGeneration, generation);
            if (previous == generation)
            {
                return false;
            }
            Interlocked.Exchange(ref foregroundArmAcknowledgedGeneration, 0);
            return true;
        }

        internal static bool TryAcknowledgeForegroundArm(int generation)
        {
            if (generation <= 0 || generation != ForegroundArmRequestedGeneration)
            {
                return false;
            }
            if (Interlocked.CompareExchange(ref foregroundArmAcknowledgedGeneration, generation, 0) != 0)
            {
                return false;
            }
            Interlocked.Increment(ref foregroundArmAcknowledgementCount);
            return true;
        }

        public static void RunSelfTest()
        {
            if (SentinelForm.StableWindowTitle != "LBB Foreground Sentinel")
            {
                throw new InvalidOperationException("The stable foreground-sentinel window title failed its self-test.");
            }
            if (ForegroundArmRequestedGeneration != 0 ||
                ForegroundArmAcknowledgedGeneration != 0 ||
                ForegroundArmRequestCount != 0 ||
                ForegroundArmAcknowledgementCount != 0 ||
                RecordForegroundArmRequest(0) ||
                !RecordForegroundArmRequest(41) ||
                ForegroundArmRequestedGeneration != 41 ||
                ForegroundArmRequestCount != 1 ||
                RecordForegroundArmRequest(41) ||
                ForegroundArmRequestCount != 2 ||
                TryAcknowledgeForegroundArm(40) ||
                !TryAcknowledgeForegroundArm(41) ||
                TryAcknowledgeForegroundArm(41) ||
                ForegroundArmAcknowledgedGeneration != 41 ||
                ForegroundArmAcknowledgementCount != 1 ||
                !RecordForegroundArmRequest(42) ||
                ForegroundArmRequestedGeneration != 42 ||
                ForegroundArmAcknowledgedGeneration != 0 ||
                ForegroundArmRequestCount != 3 ||
                ForegroundArmAcknowledgementCount != 1 ||
                !TryAcknowledgeForegroundArm(42) ||
                ForegroundArmAcknowledgedGeneration != 42 ||
                ForegroundArmAcknowledgementCount != 2)
            {
                throw new InvalidOperationException("The foreground-arm generation state machine failed its self-test.");
            }
            RecordForegroundArmLeftMouseDown();
            RecordForegroundArmLeftMouseUp();
            if (ForegroundArmLeftMouseDownCount != 1 || ForegroundArmLeftMouseUpCount != 1)
            {
                throw new InvalidOperationException("The foreground-arm input-attempt counters failed their self-test.");
            }
            MarkForegroundArmButtonEnabled();
            if (!ForegroundArmButtonEnabled)
            {
                throw new InvalidOperationException("The foreground-arm button-enabled receipt failed its self-test.");
            }
        }

        internal static void IncrementSentinelActivated()
        {
            Interlocked.Increment(ref sentinelActivated);
        }

        internal static void IncrementSentinelDeactivated()
        {
            Interlocked.Increment(ref sentinelDeactivated);
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
                        target.Shown += delegate
                        {
                            backdrop.Bounds = Rectangle.Inflate(target.Bounds, 28, 28);
                            target.BringToFront();
                        };
                        Application.Run(target);
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
Add-Type -TypeDefinition $fixtureSource -Language CSharp -ReferencedAssemblies @(
    "System.Windows.Forms",
    "System.Drawing",
    "System.Web.Extensions"
)
$fixtureRuntimeType = ("$fixtureNamespace.FixtureRuntime" -as [type])
if ($null -eq $fixtureRuntimeType) {
    throw "The isolated Windows fixture runtime type did not load."
}

if ($SelfTest) {
    $sentinelSourceStart = $fixtureSource.IndexOf(
        'internal sealed class SentinelForm : Form',
        [StringComparison]::Ordinal
    )
    $sentinelSourceEnd = $fixtureSource.IndexOf(
        'internal sealed class OccluderForm : Form',
        $sentinelSourceStart + 1,
        [StringComparison]::Ordinal
    )
    if ($sentinelSourceStart -lt 0 -or $sentinelSourceEnd -le $sentinelSourceStart) {
        throw "The foreground-sentinel source boundary failed its self-test."
    }
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
    if ($sentinelSource.IndexOf('LBB Windows Acceptance - ACTION REQUIRED', [StringComparison]::Ordinal) -ge 0 -or
        $sentinelSource.IndexOf('LBB Windows Acceptance - ARMED', [StringComparison]::Ordinal) -ge 0) {
        throw "The foreground-sentinel top-level title must not encode its arm state."
    }
    $fixtureRuntimeType::RunSelfTest()
    Write-Output "Windows computer-use fixture self-test passed."
    return
}

$fixtureRuntimeType::Run($evidenceRoot, $ShowOccluder.IsPresent)
