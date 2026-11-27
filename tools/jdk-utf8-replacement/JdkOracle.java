// SPDX-License-Identifier: Apache-2.0
//
// Licensed under the Apache License, Version 2.0. See LICENSE-APACHE and NOTICE
// at the repository root.
//
// #190: the outside oracle of verify_rule.py's `check_properties`.
//
// Reads byte rows (`hex` per line, empty for no bytes) on stdin and writes
// `new String(bytes, StandardCharsets.UTF_8)` of each back as hex code points
// (`U+XXXX` separated by spaces), so the script can compare its independent
// rule with the JDK on inputs it made for properties, not only on the table:
//
//   python3 tools/jdk-utf8-replacement/verify_rule.py | \
//     java tools/jdk-utf8-replacement/JdkOracle.java

import java.io.BufferedReader;
import java.io.InputStreamReader;
import java.io.PrintWriter;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.List;

public final class JdkOracle {

    private JdkOracle() {}

    public static void main(String[] args) throws Exception {
        BufferedReader in = new BufferedReader(new InputStreamReader(System.in, StandardCharsets.UTF_8));
        PrintWriter out = new PrintWriter(System.out, true, StandardCharsets.UTF_8);
        String line;
        while ((line = in.readLine()) != null) {
            String[] parts = line.isBlank() ? new String[0] : line.trim().split(" ");
            List<Byte> values = new ArrayList<>();
            for (String part : parts) {
                values.add((byte) Integer.parseInt(part, 16));
            }
            byte[] bytes = new byte[values.size()];
            for (int i = 0; i < bytes.length; i++) {
                bytes[i] = values.get(i);
            }
            String decoded = new String(bytes, StandardCharsets.UTF_8);
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
            if (text.isEmpty()) {
                text.append("-");
            }
            out.println(text);
        }
    }
}
