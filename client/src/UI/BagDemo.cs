using System;
using System.Linq;
using System.Threading.Tasks;
using Godot;
using Mmo.Content;
using Mmo.Gameplay;
using Mmo.Net;

namespace Mmo.UI
{
    /// <summary>
    /// <c>--bag-demo</c> (dev builds, docs/design/items.md): once in the world, empties the bag,
    /// grants some items and works the bag window through a split, a merge, a move, a swap and a
    /// destroy with the same clicks the mouse makes, logging the bag after each and pausing so
    /// captures can see each step.
    /// </summary>
    public partial class BagDemo : Node
    {
        private const double PauseS = 1.5;
        private readonly Session session;
        private readonly BagWindow bag;
        private bool started;

        public BagDemo(Session session, BagWindow bag)
        {
            this.session = session;
            this.bag = bag;
            Name = "BagDemo";
        }

        public override void _Process(double delta)
        {
            if (started || session.Me is null)
                return;
            started = true;
            _ = Run();
        }

        private async Task Run()
        {
            var r = session.Conn!.Reducers;
            await Wait(1.0);
            bag.Open = true;
            await Wait(0.2);
            foreach (var s in session.Bag.ToList())
                r.DestroyItem(s.Id, s.Count);
            await Until("an empty bag", () => !session.Bag.Any());
            foreach (var (id, n) in new[] { ("saltreed", 1500u), ("duskberry_tart", 30u), ("reedcutter_knife", 1u), ("tinstone", 240u), ("driftpine_log", 60u) })
                r.DevGrantItem(id, n);
            await Step("granted", () => { }, () => session.Bag.Count() == 6);

            await Step("split half of slot 1 into slot 12", async () =>
            {
                bag.ClickSlot(1, shift: true);
                await Wait(PauseS);
                bag.ClickSlot(12, shift: false);
            }, () => At(12) is { Count: 250 });
            await Step("merged slot 12 back into slot 1", async () =>
            {
                bag.ClickSlot(12, shift: false);
                await Wait(PauseS);
                bag.ClickSlot(1, shift: false);
            }, () => At(12) is null && At(1) is { Count: 500 });
            await Step("moved the knife from slot 3 to slot 9", async () =>
            {
                bag.ClickSlot(3, shift: false);
                await Wait(PauseS);
                bag.ClickSlot(9, shift: false);
            }, () => At(9) is { Def: 200 });
            await Step("swapped the tarts in slot 2 with the saltreed in slot 0", async () =>
            {
                bag.ClickSlot(2, shift: false);
                await Wait(PauseS);
                bag.ClickSlot(0, shift: false);
            }, () => At(0) is { Def: 100 });
            await Step("destroyed the tinstone in slot 4", async () =>
            {
                bag.DestroySlot(4);
                await Wait(PauseS);
                bag.ConfirmDestroy();
            }, () => At(4) is null);
            session.Log("bag demo: done");
        }

        private BagStack? At(ushort slot) => session.Bag.Where(s => s.Slot == slot).Select(s => (BagStack?)s).FirstOrDefault();

        private async Task Step(string what, Func<Task> act, Func<bool> done)
        {
            await act();
            await Until(what, done);
            session.Log($"bag demo: {what}: {Describe()}");
            await Wait(PauseS);
        }

        private Task Step(string what, Action act, Func<bool> done) => Step(what, () => { act(); return Task.CompletedTask; }, done);

        private string Describe() =>
            string.Join("  ", session.Bag.OrderBy(s => s.Slot).Select(s => $"{s.Slot}:{GameContent.Item(s.Def)?.Name ?? "?"}×{s.Count}"));

        private async Task Wait(double secs) => await ToSignal(GetTree().CreateTimer(secs), SceneTreeTimer.SignalName.Timeout);

        private async Task Until(string what, Func<bool> cond, double timeoutS = 5)
        {
            ulong end = Time.GetTicksMsec() + (ulong)(timeoutS * 1000);
            while (!cond())
            {
                if (Time.GetTicksMsec() > end)
                {
                    GD.PushError($"bag demo: timed out waiting for {what}: {Describe()}");
                    return;
                }
                await ToSignal(GetTree(), SceneTree.SignalName.ProcessFrame);
            }
        }
    }
}
