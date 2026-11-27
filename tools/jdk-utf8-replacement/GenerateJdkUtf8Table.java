// SPDX-License-Identifier: Apache-2.0
//
// Licensed under the Apache License, Version 2.0. See LICENSE-APACHE and NOTICE
// at the repository root.
//
// #190: writes the fixed table of `new String(bytes, StandardCharsets.UTF_8)`
// outputs for crates/fraktor-actor/tests/byte_string_spec.rs.
//
// The table is the independent expected value of `ByteString::decode_string`:
// the test compares both the decode under test and the test-side reference
// decoder with it, and neither is used to make it. Run it with the JDK the
// design names (docs/design/issue-190-bytestring.md, section 8.2):
//
//   java tools/jdk-utf8-replacement/GenerateJdkUtf8Table.java > /tmp/jdk-utf8-table.tsv
//   java tools/jdk-utf8-replacement/VerifyJdkUtf8Rule.java            # optional, the counts
//
// The output holds one `#` comment line per group and one line per case:
// tab-separated input hex (empty for the empty input), the output as code
// points in `U+XXXX` (empty for an empty output), and `group / input`. The file
// in the repository has a comment header in front of it and nothing else.

import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.List;

public final class GenerateJdkUtf8Table {

    private record Case(String name, byte[] bytes) {}

    private GenerateJdkUtf8Table() {}

    private static byte[] bytes(int... values) {
        byte[] out = new byte[values.length];
        for (int i = 0; i < values.length; i++) {
            out[i] = (byte) values[i];
        }
        return out;
    }

    private static List<Case> cases() {
        List<Case> out = new ArrayList<>();
        // One case per input: the name joins the group and the input.
        add(out);
        add(out, "base", "61");
        add(out, "base", "62");
        add(out, "base", "63");
        add(out, "base", "C3 A9");
        add(out, "base", "E2 82 AC");
        add(out, "base", "F0 9F 98 80");
        add(out, "base", "C0 80");
        add(out, "base", "C1 BF");
        add(out, "base", "E0 80 80");
        add(out, "base", "E0 9F BF");
        add(out, "base", "E0 A0 80");
        add(out, "base", "F0 80 80 80");
        add(out, "base", "F0 8F BF BF");
        add(out, "base", "F0 90 80 80");
        add(out, "base", "F4 90 80 80");
        add(out, "base", "F4 8F BF BF");
        add(out, "base", "F5 80 80 80");
        add(out, "base", "C3");
        add(out, "base", "E2 82");
        add(out, "base", "F0 9F 98");
        add(out, "base", "F0 9F 98 41");
        add(out, "base", "80");
        add(out, "base", "80 80");
        add(out, "base", "BF");
        add(out, "base", "F5");
        add(out, "base", "F8 88 80 80 80");
        add(out, "base", "FE FF");
        add(out, "base", "E1 41 80");
        add(out, "base", "E2 82 41");
        add(out, "base", "61 80 62 F0 9F 98 80");
        add(out, "base", "E1 80 E2 F0 91 92 F1 BF 41");
        // Output with a tab or a newline: the table spells them U+0009 and
        // U+000A, so the tab stays its field separator.
        add(out, "base", "ED A0 09");
        add(out, "base", "ED A0 09 41");
        add(out, "base", "ED A0 0A");
        add(out, "base", "ED A0 0A 41");

        add(out, "ed three bytes", "ED A0 80");
        add(out, "ed three bytes", "ED A0 BD");
        add(out, "ed three bytes", "ED AF BF");
        add(out, "ed three bytes", "ED B0 80");
        add(out, "ed three bytes", "ED BF BF");
        add(out, "ed three bytes", "61 ED A0 80 62");
        add(out, "ed three bytes", "C3 A9 ED BF BF C3 A9");
        add(out, "ed three bytes", "ED A0 80 ED B0 80");
        add(out, "ed three bytes", "ED A0 80 ED A0 80");
        add(out, "ed three bytes", "ED A0 80 80");
        add(out, "ed three bytes", "ED A0 80 80 80");
        add(out, "ed three bytes", "ED A0 80 80 80 80");
        add(out, "ed three bytes", "ED A0 80 80 41");
        add(out, "ed three bytes", "ED A0 80 80 80 41");

        add(out, "ed cut at two bytes", "ED A0");
        add(out, "ed cut at two bytes", "ED BF");
        add(out, "ed cut at two bytes", "61 ED A0");
        add(out, "ed cut at two bytes", "ED BF 80 80");

        add(out, "ed three bytes cut before a non-continuation byte", "ED A0 41");
        add(out, "ed three bytes cut before a non-continuation byte", "ED BF 41");
        add(out, "ed three bytes cut before a non-continuation byte", "ED A0 C3 A9");
        add(out, "ed three bytes cut before a non-continuation byte", "ED A0 ED A0 80");
        add(out, "ed three bytes cut before a non-continuation byte", "ED A0 80 ED BF BF ED AF 41");
        add(out, "ed three bytes cut before a non-continuation byte", "ED A0 C3");
        add(out, "ed three bytes cut before a non-continuation byte", "ED A0 C0");
        add(out, "ed three bytes cut before a non-continuation byte", "ED A0 00");
        add(out, "ed three bytes cut before a non-continuation byte", "ED A0 7F");
        add(out, "ed three bytes cut before a non-continuation byte", "ED A0 F5");
        add(out, "ed three bytes cut before a non-continuation byte", "ED A0 ED");
        add(out, "ed three bytes cut before a non-continuation byte", "ED A0 41 42");
        add(out, "ed three bytes cut before a non-continuation byte", "ED A0 C0 80");

        add(out, "ed three bytes cut at two bytes before a non-continuation byte", "ED BF 41");
        add(out, "ed three bytes cut at two bytes before a non-continuation byte", "ED A0 C0");
        add(out, "ed three bytes cut at two bytes before a non-continuation byte", "ED A0 00");
        add(out, "ed three bytes cut at two bytes before a non-continuation byte", "ED A0 80 41");

        add(out, "before an ed unit", "E1 80 ED A0 80 80");
        add(out, "before an ed unit", "E0 80 80 ED A0 80");
        add(out, "before an ed unit", "F0 9F 98 ED A0 80 80");
        add(out, "before an ed unit", "E2 82 ED A0 80");
        add(out, "before an ed unit", "E2 82 ED A0");
        add(out, "before an ed unit", "F0 9F 98 ED A0 80");
        add(out, "before an ed unit", "F0 9F 98 ED A0");

        add(out, "next to the surrogates", "ED 9F BF");
        add(out, "next to the surrogates", "ED 80 80");
        add(out, "next to the surrogates", "ED 9F 80");
        add(out, "next to the surrogates", "EE 80 80");
        add(out, "next to the surrogates", "EE 9F BF");
        add(out, "next to the surrogates", "ED 9F BF ED A0 80");
        add(out, "next to the surrogates", "ED 9F 80 ED 80 80");

        add(out, "base", "ED A0 80");
        add(out, "base", "ED A0");
        add(out, "base", "FF");
        add(out, "base", "EF BF BD");
        add(out, "base", "61 EF BF BD 62");
        add(out, "base", "80 EF BF BD");
        add(out, "base", "EF BF BD ED A0 80");
        add(out, "base", "C3 A9 ED A0 80 C0 80 E0 80 80 F0 9F 98 41 80");
        add(out, "base", "F4 90 80 80 ED BF BF ED 9F BF ED 80 80 EE 80 80");
        return out;
    }

    private static void add(List<Case> out, String... groupsAndInputs) {
        if (groupsAndInputs.length == 0) {
            out.add(new Case("empty", new byte[0]));
            return;
        }
        String group = groupsAndInputs[0];
        for (int i = 1; i < groupsAndInputs.length; i++) {
            out.add(new Case(group + " / " + groupsAndInputs[i], parse(groupsAndInputs[i])));
        }
    }

    private static byte[] parse(String hex) {
        if (hex.isEmpty()) {
            return new byte[0];
        }
        String[] parts = hex.split(" ");
        int[] values = new int[parts.length];
        for (int i = 0; i < parts.length; i++) {
            values[i] = Integer.parseInt(parts[i], 16);
        }
        return bytes(values);
    }

    private static String hex(byte[] input) {
        if (input.length == 0) {
            return "";
        }
        StringBuilder text = new StringBuilder();
        for (byte b : input) {
            if (!text.isEmpty()) {
                text.append(' ');
            }
            text.append(String.format("%02X", b & 0xFF));
        }
        return text.toString();
    }

    private static String codePoints(String decoded) {
        StringBuilder text = new StringBuilder();
        int i = 0;
        while (i < decoded.length()) {
            int cp = decoded.codePointAt(i);
            if (!text.isEmpty()) {
                text.append(' ');
            }
            text.append(String.format("U+%04X", cp));
            i += Character.charCount(cp);
        }
        return text.toString();
    }

    public static void main(String[] args) {
        String lastGroup = null;
        for (Case c : cases()) {
            String group = c.name().contains(" / ") ? c.name().substring(0, c.name().indexOf(" / ")) : c.name();
            if (group.equals("empty")) {
                System.out.println("\t\t" + c.name());
                continue;
            }
            if (!group.equals(lastGroup)) {
                System.out.println("# " + group);
                lastGroup = group;
            }
            String decoded = new String(c.bytes(), StandardCharsets.UTF_8);
            System.out.println(hex(c.bytes()) + "\t" + codePoints(decoded) + "\t" + c.name());
        }
    }
}
