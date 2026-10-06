// Offline reference generator. Pass pinned kotlin-multiplatform-diff-jvm 1.3.0 and
// Kotlin stdlib on javac/java classpath; library is never used by runtime or tests.
import io.github.petertrr.diffutils.DiffUtils;
import io.github.petertrr.diffutils.patch.Delta;
import java.util.ArrayList;
import java.util.List;

public class BBCodeDiffOracle {
    private static String units(String value) {
        var output = new StringBuilder();
        for (int i = 0; i < value.length(); i++) output.append(String.format("%04x", (int) value.charAt(i)));
        return output.toString();
    }
    private static void emit(String source, String target) {
        var deltas = DiffUtils.diffInline(source, target).getDeltas();
        System.out.print(units(source) + "\t" + units(target) + "\t" + deltas.size());
        if (deltas.size() == 1) {
            Delta<String> delta = deltas.get(0);
            int sourceStart = delta.getSource().position;
            int targetStart = delta.getTarget().position;
            int sourceLength = delta.getSource().lines.stream().mapToInt(String::length).sum();
            int targetLength = delta.getTarget().lines.stream().mapToInt(String::length).sum();
            System.out.print("\t" + delta.getType() + "\t" + sourceStart + "\t" + (sourceStart + sourceLength)
                + "\t" + targetStart + "\t" + (targetStart + targetLength));
        }
        System.out.println();
    }
    public static void main(String[] args) {
        List<String> texts = new ArrayList<>();
        texts.add("");
        for (int length = 1; length <= 3; length++) {
            int count = (int) Math.pow(3, length);
            for (int i = 0; i < count; i++) {
                int value = i;
                var text = new StringBuilder();
                for (int j = 0; j < length; j++) { text.append((char) ('a' + value % 3)); value /= 3; }
                texts.add(text.toString());
            }
        }
        texts.addAll(List.of("aaa", "aaXaa", "abcabc", "abXcabYc", "a\r\nb", "a\nb", "中😀文", "😀😀", "\ud800", "\udc00", "x\ud800y", "\u0000"));
        for (String source : texts) for (String target : texts) emit(source, target);
    }
}
