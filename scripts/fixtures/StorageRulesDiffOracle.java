// Offline reference generator; only JDK needed. Never linked into app/tests.
import java.util.ArrayList;
import java.util.Comparator;
import java.util.List;
import java.util.Random;

public class StorageRulesDiffOracle {
    private static String units(String value) {
        var text = new StringBuilder("\"");
        for (int i = 0; i < value.length(); i++) text.append(String.format("%04x", (int) value.charAt(i)));
        return text.append('"').toString();
    }
    private static String strings(List<String> values) {
        return "[" + String.join(",", values.stream().map(StorageRulesDiffOracle::units).toList()) + "]";
    }
    private static String integers(List<Integer> values) { return values.toString(); }
    public static void main(String[] args) {
        var random = new Random(0x4a4e49534f52544cL);
        var alphabet = new char[] { 'a', 'b', '0', '9', '.', '/', '\u00ff', '\u0100', '\ud800', '\ud83d', '\ude00', '\ue000' };
        for (int test = 0; test < 512; test++) {
            int count = random.nextInt(33);
            var names = new ArrayList<String>();
            var times = new ArrayList<Long>();
            var kinds = new ArrayList<String>();
            for (int i = 0; i < count; i++) {
                var name = new StringBuilder();
                for (int j = random.nextInt(15); j > 0; j--) name.append(alphabet[random.nextInt(alphabet.length)]);
                names.add(i > 0 && random.nextInt(4) == 0 ? names.get(i - 1) : name.toString());
                times.add(switch (random.nextInt(6)) { case 0 -> Long.MIN_VALUE; case 1 -> Long.MAX_VALUE; case 2 -> 0L; case 3 -> 10L; default -> random.nextLong(); });
                kinds.add(random.nextBoolean() ? "directory" : "zip");
            }
            int limit = random.nextInt(40) - 3;
            var order = new ArrayList<Integer>();
            for (int i = 0; i < count; i++) order.add(i);
            order.sort(Comparator.comparing(names::get));
            System.out.println("{\"operation\":\"names\",\"utf16\":" + strings(names) + ",\"expected\":" + integers(order) + "}");
            order.clear(); for (int i = 0; i < count; i++) order.add(i);
            order.sort(Comparator.<Integer>comparingLong(times::get).reversed());
            var deletes = new ArrayList<Integer>();
            for (int i = 0; i < count; i++) if (i >= limit) deletes.add(order.get(i));
            System.out.println("{\"operation\":\"cleanup\",\"modified\":" + times + ",\"maxItems\":" + limit + ",\"expected\":" + integers(deletes) + "}");
            order.clear();
            for (int i = 0; i < count; i++) if (kinds.get(i).equals("directory")) order.add(i);
            for (int i = 0; i < count; i++) if (!kinds.get(i).equals("directory")) order.add(i);
            order.sort(Comparator.comparing(names::get));
            int maximum = Math.max(0, limit);
            var selected = order.subList(0, Math.min(maximum, count));
            System.out.println("{\"operation\":\"candidates\",\"utf16\":" + strings(names) + ",\"kinds\":[" + String.join(",", kinds.stream().map(kind -> "\"" + kind + "\"").toList()) + "],\"maxItems\":" + maximum + ",\"expected\":" + integers(selected) + ",\"limitExceeded\":" + (count > maximum) + "}");
        }
    }
}
