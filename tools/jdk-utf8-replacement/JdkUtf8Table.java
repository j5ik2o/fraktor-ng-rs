// SPDX-License-Identifier: Apache-2.0
//
// Licensed under the Apache License, Version 2.0. See LICENSE-APACHE and NOTICE
// at the repository root.
//
// #190: the JDK side of tools/jdk-utf8-replacement/check_counts.py: answers
// `new String(bytes, StandardCharsets.UTF_8)` for byte rows read on stdin
// (`hex` per line, empty for no bytes) with the code points (`U+XXXX`
// separated by spaces, `-` for none), one line per row, as it goes, so the
// 8,421,504 rows of the all family fit in a pipe:
//
//   ... | java tools/jdk-utf8-replacement/JdkUtf8Table.java

import java.io.BufferedReader;
import java.io.InputStreamReader;
import java.io.PrintWriter;
import java.nio.charset.StandardCharsets;

public final class JdkUtf8Table {

    private JdkUtf8Table() {}

    public static void main(String[] args) throws Exception {
        BufferedReader in = new BufferedReader(new InputStreamReader(System.in, StandardCharsets.US_ASCII));
        PrintWriter out = new PrintWriter(System.out, false, StandardCharsets.US_ASCII);
        String line;
        while ((line = in.readLine()) != null) {
            String trimmed = line.trim();
            byte[] bytes;
            if (trimmed.isEmpty()) {
                bytes = new byte[0];
            } else {
                String[] parts = trimmed.split(" ");
                bytes = new byte[parts.length];
                for (int i = 0; i < parts.length; i++) {
                    bytes[i] = (byte) Integer.parseInt(parts[i], 16);
                }
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
            out.println(text.isEmpty() ? "-" : text);
        }
        out.flush();
    }
}
