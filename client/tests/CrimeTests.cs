using System.Numerics;
using Mmo.Content;
using Mmo.Gameplay;
using Mmo.Sim;
using Xunit;

namespace Mmo.Tests
{
    /// <summary>Evidence kinds, the report prompt and the record line (docs/design/crime.md).</summary>
    public class CrimeTests
    {
        private const long T0 = 1_800_000_000_000_000;
        private const long Day = 86_400_000_000;

        [Fact]
        public void KindsHaveNamesAndPointsFromContent()
        {
            Assert.Equal("Footprints", Crime.Name(EvidenceKind.Footprints));
            Assert.Equal("Blood Pool", Crime.Name(EvidenceKind.BloodPool));
            Assert.Equal(3, Crime.Points(EvidenceKind.Footprints));
            Assert.Equal(1, Crime.Points(EvidenceKind.Bloodstain));
            Assert.Equal(10, Crime.Points(EvidenceKind.BloodPool));
            Assert.Equal("bloodmarked", GameContent.Statuses[GameContent.Crime.Mark].Id);
        }

        [Fact]
        public void EvidenceFadesAfterADay()
        {
            Assert.Equal(T0 + Day, Crime.FadesAt(T0));
            Assert.False(Crime.Faded(T0, T0 + Day - 1));
            Assert.True(Crime.Faded(T0, T0 + Day));
        }

        [Fact]
        public void JReportsTheNearestUnfadedEvidenceInReach()
        {
            var at = new Vector3(0, 5, 0);
            var far = new EvidenceInfo(1, EvidenceKind.BloodPool, new Vector3(4.5f, 5, 0), T0);
            var old = new EvidenceInfo(2, EvidenceKind.Bloodstain, new Vector3(0.5f, 5, 0), T0 - Day);
            var near = new EvidenceInfo(3, EvidenceKind.Footprints, new Vector3(2, 5, 1), T0);
            var nearer = new EvidenceInfo(4, EvidenceKind.Bloodstain, new Vector3(1, 5, 1), T0);
            var below = new EvidenceInfo(5, EvidenceKind.Bloodstain, new Vector3(0, 0.5f, 0), T0);
            Assert.Equal(4ul, CrimeModel.Nearest(new[] { far, old, near, nearer, below }, at, T0 + 1)?.Id);
            Assert.Equal(3ul, CrimeModel.Nearest(new[] { far, old, near }, at, T0 + 1)?.Id);
            Assert.Null(CrimeModel.Nearest(new[] { far, old, below }, at, T0 + 1));
            Assert.Equal("J: report Footprints", CrimeModel.Prompt(near));
        }

        [Fact]
        public void TheRecordLineAndSoughtTags()
        {
            Assert.Equal("", CrimeModel.RecordLine(null, 0));
            Assert.Equal("crime 11 · notoriety 1,011", CrimeModel.RecordLine(11, 1011));
            Assert.Equal("crime 50 · notoriety 61 · Sought", CrimeModel.RecordLine(50, 61));
            Assert.Equal("Mira\nSought", CrimeModel.Tagged("Mira", true));
            Assert.Equal("Mira  ·  Sought", CrimeModel.Tagged("Mira", true, "  ·  "));
            Assert.Equal("Mira", CrimeModel.Tagged("Mira", false));
        }
    }
}
