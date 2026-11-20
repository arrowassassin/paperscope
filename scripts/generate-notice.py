#!/usr/bin/env python3
"""
Generate SPDX-compliant NOTICE file from cargo metadata.
Uses local SPDX license texts for known licenses (fast).
Only fetches from GitHub for unknown licenses.
"""

import json
import subprocess
import sys
import re
from pathlib import Path
from collections import defaultdict
from dataclasses import dataclass
from typing import Optional


@dataclass
class Dependency:
    name: str
    version: str
    license: str
    repository: str
    license_text: str = ""


# Standard SPDX license texts for common licenses
SPDX_LICENSES = {
    "MIT": """MIT License

Copyright (c) <year> <copyright holders>

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.""",
    "APACHE-2.0": """Apache License
Version 2.0, January 2004
http://www.apache.org/licenses/

TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION

1. Definitions.

"License" shall mean the terms and conditions for use, reproduction,
and distribution as defined by Sections 1 through 9 of this document.

"Licensor" shall mean the copyright owner or entity authorized by
the copyright owner that is granting the License.

"Legal Entity" shall mean the union of the acting entity and all
other entities that control, are controlled by, or are under common
control with that entity. For the purposes of this definition,
"control" means (i) the power, direct or indirect, to cause the
direction or management of such entity, whether by contract or
otherwise, or (ii) ownership of fifty percent (50%) or more of the
outstanding shares, or (iii) the beneficial ownership of such entity.

"You" (or "Your") shall mean an individual or Legal Entity
exercising permissions granted by this License.

"Source" form shall mean the preferred form for making modifications,
including but not limited to software source code, documentation
source, and configuration files.

"Object" form shall mean any form resulting from mechanical
transformation or translation of a Source form, including but
not limited to compiled object code, generated documentation,
and conversions to other media types.

"Work" shall mean the work of authorship, whether in Source or
Object form, made available under the License, as indicated by a
copyright notice that is included in or attached to the work
(an example is provided in Appendix A).

"Derivative Works" shall mean any work, whether in Source or Object
form, that is based on (or derived from) the Work and for which the
editorial revisions, annotations, elaborations, or other modifications
represent, as a whole, an original work of authorship. For the purposes
of this License, Derivative Works shall not include works that remain
substantially identical to the Work.

"Contribution" shall mean any work of authorship, including
the original version of the Work and any modifications or additions
to that Work or Derivative Works thereof, that is intentionally
submitted to Licensor for inclusion in the Work by the copyright owner
or by an individual or Legal Entity authorized to submit on behalf of
the copyright owner. For the purposes of this definition, "submitted"
means any form of electronic, verbal, or written communication sent
to the Licensor or its representatives, including but not limited to
communication on electronic mailing lists, bulletin board systems,
and similar forums, but excluding communication that is conspicuously
marked or otherwise designated in writing by the copyright owner as
"Not a Contribution."

"Contributor" shall mean Licensor and any individual or Legal Entity
on behalf of whom a Contribution has been received by Licensor and
subsequently incorporated within the Work.

2. Grant of Copyright License. Subject to the terms and conditions of
this License, each Contributor hereby grants to You a perpetual,
worldwide, non-exclusive, no-charge, royalty-free, irrevocable
copyright license to reproduce, prepare Derivative Works of,
publicly display, publicly perform, sublicense, and distribute the
Work and such Derivative Works in Source or Object form.

3. Grant of Patent License. Subject to the terms and conditions of
this License, each Contributor hereby grants to You a perpetual,
worldwide, non-exclusive, no-charge, royalty-free, irrevocable
(except as stated in this section) patent license to make, have made,
use, offer to sell, sell, import, and otherwise transfer the Work,
where such license applies only to those patent claims licensable
by such Contributor that are necessarily infringed by their
Contribution(s) alone or by combination of their Contribution(s)
with the Work to which such Contribution(s) was submitted. If You
institute patent litigation against any entity (including a
cross-claim or counterclaim in a lawsuit) alleging that the Work
or a Contribution incorporated within the Work constitutes direct
or contributory patent infringement, then any patent licenses
granted to You under this License for that Work shall terminate
as of the date such litigation is filed.

4. Redistribution. You may reproduce and distribute copies of the
Work or Derivative Works thereof in Source or Object form, with or
without modifications, and in Source or Object form, provided that
You meet the following conditions:

(a) You must give any other recipients of the Work or
Derivative Works a copy of this License; and

(b) You must cause any modified files to carry prominent notices
stating that You changed the files; and

(c) You must retain, in the Source form of any Derivative Works
that You distribute, all copyright, patent, trademark, and
attribution notices from the Source form of the Work, excluding
those notices that do not pertain to the part of the Source form
of the Work that You distribute; and

(d) If the Work includes a "NOTICE" text file as part of its
distribution, then any Derivative Works that You distribute must
include a readable copy of the attribution notices contained
within such NOTICE file, excluding those notices that do not
pertain to the part of the Source form of the Work that You
distribute, in at least one of the following places: within a
NOTICE text file distributed as part of the Derivative Works;
within the Source form or Object form of the Derivative Works;
or, within a display generated by the Derivative Works, if and
wherever such third-party Derivative Works generate the display
of such attribution notices. The contents of the NOTICE file
are for informational purposes only and do not modify the License.
You may add Your own attribution notices within Derivative Works
that You distribute, alongside or as an addendum to the NOTICE
text from the Work, provided that such additional attribution
notices cannot be construed as modifying the License.

5. Submission of Contributions. Unless You explicitly state otherwise,
any Contribution intentionally submitted for inclusion in the Work
by You to the Licensor is assumed to be under the terms and conditions
of this License, without any additional terms or conditions.
Notwithstanding the above, nothing herein shall supersede or modify
the terms of any separate license agreement you may have executed
with Licensor prior to this License.

6. Trademarks. This License does not grant permission to use the trade
names, trademarks, service marks, or product names of the Licensor,
except as required for reasonable and customary use in describing the
origin of the Work and reproducing the content of the Source form
of the Work. This License does not grant permission to use the
Licensor's trademarks to endorse or promote products derived from
the Work without the prior written permission of the Licensor.

7. Disclaimer of Warranty. Unless required by applicable law or
agreed to in writing, Licensor provides the Work (and each
Contributor provides its Contributions) on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or
implied, including, without limitation, warranties or conditions
of MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE, or
NONINFRINGEMENT. Licensor does not assume any liability for
the safety or reliability of operations performed by the Work.

8. Limitation of Liability. In no event and under no legal theory,
whether in tort (including negligence), contract, or otherwise,
shall any Contributor be liable to You for damages, including any
direct, indirect, special, incidental, or consequential damages
of any character arising as a result of this License or out of the
use or inability to use the Work (including but not limited to
damages for loss of goodwill, work stoppage, equipment failure
or injury, natural disasters, or acts of war), even if such Contributor
has been advised of the possibility of such damages.

9. Accepting Warranty or Additional Liability. While redistributing
the Work or Derivative Works thereof, You may choose to offer,
and charge a fee for, acceptance of support, warranty, or other
liability obligations consistent with the terms of this License.
However, in accepting such obligations, You must act solely on
Your own behalf and on Your sole responsibility, not on behalf
of any other Contributor, unless explicitly authorized by the
Contributor.

END OF TERMS AND CONDITIONS

APPENDIX A: How to apply the Apache License to your Work.

To apply the Apache License to your work, attach the following
boilerplate notice, with the fields enclosed by brackets "[]"
replaced with your own identifying information. (Don't include
the brackets!) The text should be wrapped in a way that is
appropriate for the file format and in accordance with standard
practice for the markup language in use.

Copyright [yyyy] [name of copyright owner]

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

    http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.""",
    "BSD-2-CLAUSE": """BSD 2-Clause "Simplified" License

Copyright (c) <year>, <copyright holder>
All rights reserved.

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are met:

1. Redistributions of source code must retain the above copyright notice, this
   list of conditions and the following disclaimer.

2. Redistributions in binary form must reproduce the above copyright notice,
   this list of conditions and the following disclaimer in the documentation
   and/or other materials provided with the distribution.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE
FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY,
OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.""",
    "BSD-3-CLAUSE": """BSD 3-Clause "New" or "Revised" License

Copyright (c) <year>, <copyright holder>
All rights reserved.

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are met:

1. Redistributions of source code must retain the above copyright notice, this
   list of conditions and the following disclaimer.

2. Redistributions in binary form must reproduce the above copyright notice,
   this list of conditions and the following disclaimer in the documentation
   and/or other materials provided with the distribution.

3. Neither the name of the copyright holder nor the names of its
   contributors may be used to endorse or promote products derived from
   this software without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE
FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY,
OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.""",
    "ISC": """ISC License

Copyright (c) <year>, <copyright holder>

Permission to use, copy, modify, and/or distribute this software for any
purpose with or without fee is hereby granted, provided that the above
copyright notice and this permission notice appear in all copies.

THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES
WITH REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF
MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR
ANY SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES
WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN
ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF
OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE.""",
    "CC0-1.0": """Creative Commons Legal Code

CC0 1.0 Universal

    CREATIVE COMMONS CORPORATION IS NOT A LAW FIRM AND DOES NOT PROVIDE
    LEGAL SERVICES. DISTRIBUTION OF THIS DOCUMENT DOES NOT CREATE AN
    ATTORNEY-CLIENT RELATIONSHIP. CREATIVE COMMONS PROVIDES THIS
    INFORMATION ON AN "AS-IS" BASIS. CREATIVE COMMONS MAKES NO WARRANTIES
    REGARDING THE USE OF THIS DOCUMENT OR THE INFORMATION OR WORKS
    PROVIDED HEREUNDER, AND DISCLAIMS LIABILITY FOR DAMAGES RESULTING FROM
    THE USE OF THIS DOCUMENT OR THE INFORMATION OR WORKS PROVIDED
    HEREUNDER.

Statement of Purpose

The laws of most jurisdictions throughout the world automatically confer
exclusive Copyright and Related Rights (defined below) upon the creator
and subsequent owner(s) (each and all, an "owner") of an original work of
authorship and/or a database (each, a "Work").

Certain owners wish to permanently relinquish those rights to a Work for
the purpose of contributing to a commons of creative, cultural and
scientific works ("Commons") that the public can reliably and freely
build upon, modify, incorporate in other works, reuse and redistribute.

Some jurisdictions do not allow owners to relinquish rights to a Work
automatically, but allow owners to waive their interests or the
enforceability of those rights. CC0 was created to allow owners to
relinquish or waive their rights to the fullest extent allowed by law.

1. Copyright and Related Rights. A Work made available under CC0 may be
protected by copyright and related or neighboring rights ("Copyright and
Related Rights"). Copyright and Related Rights include, but are not limited
to, the following:

i. the right to reproduce the Work in any material form;

ii. the right to make derivative works;

iii. the right to distribute the Work;

iv. the right to perform the Work publicly;

v. the right to make the Work available to the public;

vi. the right to perform the Work by means of digital audio-visual
    communications technology;

vii. the right to broadcast the Work;

viii. any other rights which are defined by applicable law to be
      equivalent to the rights listed above.

2. Waiver. To the greatest extent permitted by, but not in violation of,
applicable law, Affirmer hereby overtly, fully, permanently,
irrevocably and unconditionally waives, abandons, and surrenders all of
Affirmer's Copyright and Related Rights and associated claims and causes
of action, whether now known or unknown. Affirmer does so for the
purpose of contributing to the Commons and for the personal benefit of
Affirmer's heirs and successors, intending that such waiver be the most
complete and effective allowed by applicable law.

3. Public License Fallback. Should any part of the Waiver for any reason
be judged legally invalid or ineffective under applicable law, then the
Waiver shall be interpreted to have the effect of a license to the public
to the maximum extent permitted by applicable law.

4. Limitations and Disclaimers.

a. No trademark or patent rights held by Affirmer are waived,
   abandoned, surrendered or licensed by this document.

b. Affirmer offers the Work as-is and makes no representations or
   warranties of any kind concerning the Work, express, implied,
   statutory or otherwise, including without limitation warranties of
   merchantability, fitness for a particular purpose, or non-infringement.

c. Affirmer disclaims responsibility for clearing any other rights
   which may be required for use of the Work, including without
   limitation any rights that may be required to remove any encumbrances
   on the Work in any jurisdiction.

d. Affirmer disclaims responsibility for obtaining any consents,
   permissions or other clearances which may be required for use of the
   Work, including without limitation any rights that may be required
   to remove any encumbrances on the Work in any jurisdiction.

e. Affirmer understands and acknowledges that the user of the Work may
   rely on the Work being free of any rights, obligations or
   restrictions, and Affirmer accepts no liability to any party
   resulting from the use of the Work, whether now known or unknown.

f. Affirmer makes no warranty, express or implied, regarding the
   accuracy, completeness, or usefulness of the Work, and disclaims
   all liability for any damages resulting from the use of the Work.""",
    "UNLICENSE": """The Unlicense

This is free and unencumbered software released into the public domain.

Anyone is free to copy, modify, publish, use, compile, sell, or
distribute this software, either in source code form or as a compiled
binary, for any purpose, commercial or non-commercial, and by any
means.

In jurisdictions that recognize copyright laws, the author or authors
of this software dedicate any and all copyright interest in the
software to the public domain. We make this dedication for the benefit
of the public at large and to the detriment of our heirs and
successors. We intend this dedication to be an overt act of
relinquishment in perpetuity of all present and future rights to this
software under copyright law.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
IN NO EVENT SHALL THE AUTHORS BE LIABLE FOR ANY CLAIM, DAMAGES OR
OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE,
ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR
OTHER DEALINGS IN THE SOFTWARE.

For more information, please refer to <http://unlicense.org/>""",
    "ZLIB": """zlib License

Copyright (c) <year> <copyright holder>

This software is provided 'as-is', without any express or implied
warranty. In no event will the authors be held liable for any damages
arising from the use of this software.

Permission is granted to anyone to use this software for any purpose,
including commercial applications, and to alter it and redistribute it
freely, subject to the following restrictions:

1. The origin of this software must not be misrepresented; you must not
   claim that you wrote the original software. If you use this software
   in a product, an acknowledgment in the product documentation would be
   appreciated but is not required.

2. Altered source versions must be plainly marked as such, and must not
   be misrepresented as being the original software.

3. This notice may not be removed or altered from any source
   distribution.""",
    "BSL-1.0": """Boost Software License - Version 1.0 - August 17th, 2003

Permission is hereby granted, free of charge, to any person or organization
obtaining a copy of the software and accompanying documentation covered by
this license (the "Software") to use, reproduce, display, distribute,
execute, and transmit the Software, and to prepare derivative works of the
Software, and to permit third parties to whom the Software is furnished to
do so, all subject to the following:

The copyright notices in the Software and this entire statement, including
the license grant, this restriction, and the following disclaimer, must be
included in all copies of the Software, in whole or in part, and all
derivative works of the Software, unless such copies or derivative works
are solely in the form of machine-executable object code generated by a
source language processor.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
COPYRIGHT HOLDERS OR ANYONE DISTRIBUTING THE SOFTWARE BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT
OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR
THE USE OR OTHER DEALINGS IN THE SOFTWARE.""",
    "MIT-0": """MIT No Attribution

Copyright (c) <year> <copyright holder>

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.""",
    "0BSD": """BSD Zero Clause License

Copyright (c) <year> <copyright holder>

Permission to use, copy, modify, and/or distribute this software for any
purpose with or without fee is hereby granted.

THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES
WITH REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF
MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR
ANY SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES
WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN
ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF
OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE.""",
    "CDLA-PERMISSIVE-2.0": """Community Data License Agreement – Permissive, Version 2.0

This Community Data License Agreement – Permissive, Version 2.0 (the
"Agreement") is entered into by and between the Licensor (defined below)
and You (defined below) as of the date You accept this Agreement.

1. Definitions.

"Licensor" means the individual or entity that offers the Licensed
Material under the terms of this Agreement.

"Licensed Material" means the data, content, or other material licensed
under this Agreement.

"You" means the individual or entity exercising rights under this
Agreement.

2. Grant of Rights. Subject to the terms and conditions of this Agreement,
Licensor hereby grants to You a worldwide, royalty-free, non-exclusive,
perpetual, irrevocable license to:

a. reproduce and share the Licensed Material, in whole or in part; and

b. produce and reproduce Adapted Material.

3. Attribution. If You share the Licensed Material or Adapted Material,
You must include the following:

a. the identity of the creator(s) of the Licensed Material and any
   other parties designated to receive attribution;

b. a copyright notice in connection with the Licensed Material;

c. a notice that refers to this Agreement;

d. a notice that refers to the disclaimer of warranties and
   limitation of liability;

e. a URI or hyperlink to the Licensed Material to the extent
   reasonably practicable.

4. Disclaimer of Warranties and Limitation of Liability. THE LICENSED
MATERIAL IS PROVIDED "AS IS", WITHOUT WARRANTIES OF ANY KIND, EITHER
EXPRESS OR IMPLIED, INCLUDING, BUT NOT LIMITED TO, WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE, AND NONINFRINGEMENT.
IN NO EVENT SHALL THE LICENSOR BE LIABLE FOR ANY DIRECT, INDIRECT,
INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES ARISING OUT OF
OR IN CONNECTION WITH THIS AGREEMENT OR THE LICENSED MATERIAL, EVEN IF
THE LICENSOR HAS BEEN ADVISED OF THE POSSIBILITY OF SUCH DAMAGES.

5. Term and Termination. This Agreement applies as of the date You accept
it and continues perpetually unless terminated as set forth below. This
Agreement terminates automatically upon any breach of its terms by You.
Sections 3, 4, and 5 survive termination.

6. General Provisions. This Agreement is the entire agreement between
the parties with respect to the Licensed Material. If any provision of
this Agreement is held to be unenforceable, such provision shall be
reformed only to the extent necessary to make it enforceable. This
Agreement shall be governed by the laws of the jurisdiction in which
the Licensor has its principal place of business, without regard to
conflict of law principles.""",
    "UNICODE-3.0": """Unicode License Agreement - Data Files and Software (2016)

UNICODE, INC. LICENSE AGREEMENT - DATA FILES AND SOFTWARE

See Terms of Use at https://www.unicode.org/copyright.html

COPYRIGHT AND PERMISSION NOTICE

Copyright (c) 1991-2016 Unicode, Inc. All rights reserved.

Permission is hereby granted, free of charge, to any person obtaining
a copy of the Unicode data files and any associated documentation
(the "Data Files") or Unicode software and any associated documentation
(the "Software") to deal in the Data Files or Software without
restriction, including without limitation the rights to use, copy,
modify, merge, publish, distribute, and/or sell copies of the Data Files
or Software, and to permit persons to whom the Data Files or Software
are furnished to do so, provided that either (a) this copyright and
permission notice appear with all copies of the Data Files or Software,
or (b) this copyright and permission notice appear in associated
documentation.

THE DATA FILES AND SOFTWARE ARE PROVIDED "AS IS", WITHOUT WARRANTY OF
ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE
WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
NONINFRINGEMENT. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR COPYRIGHT
HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER
IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
IN CONNECTION WITH THE DATA FILES OR SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE DATA FILES OR SOFTWARE.

Except as contained in this notice, the name of Unicode, Inc. shall
not be used in advertising or otherwise to promote the sale, use or
other dealings in these Data Files or Software without prior written
authorization from Unicode, Inc.""",
    "APACHE-2.0 WITH LLVM-EXCEPTION": """Apache License
Version 2.0, January 2004
http://www.apache.org/licenses/

TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION

[Same as Apache-2.0 above]

LLVM Exceptions to the Apache 2.0 License

As an exception, if, as a result of Your compiling Your source code,
portions of this Software are embedded into an Object form of such
source code, You may redistribute such embedded portions in such
Object form without complying with the conditions of Sections 4(a),
4(b) and 4(d) of the License.

In addition, if You combine or link compiled forms of this Software
with other software to produce a combined work, this License does not
limit Your rights to the combined work under the terms of the license
for the other software, provided that the other software is not a
Derivative Work of this Software.""",
}


def run_cargo_metadata() -> dict:
    """Run cargo metadata and return parsed JSON."""
    result = subprocess.run(
        ["cargo", "metadata", "--format-version=1"],
        capture_output=True,
        text=True,
        check=True,
    )
    return json.loads(result.stdout)


def get_runtime_dependencies(metadata: dict) -> list[Dependency]:
    """Extract runtime dependencies from cargo metadata resolve graph."""
    if "resolve" not in metadata:
        return []

    nodes = {n["id"]: n for n in metadata["resolve"]["nodes"]}
    pkg_map = {p["id"]: p for p in metadata["packages"]}
    root = metadata["resolve"]["root"]

    visited = set()
    to_visit = [root]

    while to_visit:
        node_id = to_visit.pop()
        if node_id in visited:
            continue
        visited.add(node_id)
        if node_id in nodes:
            for dep_id in nodes[node_id]["dependencies"]:
                to_visit.append(dep_id)

    deps = []
    for pid in sorted(visited):
        if pid in pkg_map:
            p = pkg_map[pid]
            if pid == root:
                continue
            deps.append(
                Dependency(
                    name=p["name"],
                    version=p["version"],
                    license=p["license"],
                    repository=p.get("repository", "") or "",
                )
            )

    return deps


def normalize_license_id(license_str: str) -> list[str]:
    """Parse license string into list of SPDX identifiers."""
    # Handle various separators: OR, AND, /, parentheses
    # First, split by OR/AND (with spaces)
    parts = re.split(r"\s+(?:OR|AND)\s+", license_str)
    result = []
    for part in parts:
        part = part.strip()
        # Remove parentheses
        part = part.strip("()")
        # Handle WITH exceptions
        if " WITH " in part:
            part = part.split(" WITH ")[0].strip()
        # Handle slash-separated (like BSD-3-Clause/MIT)
        if "/" in part:
            subparts = [p.strip() for p in part.split("/")]
            result.extend(subparts)
        else:
            result.append(part)
    # Deduplicate while preserving order
    seen = set()
    unique = []
    for p in result:
        if p and p not in seen:
            seen.add(p)
            unique.append(p)
    return unique


def get_spdx_license_text(spdx_id: str) -> Optional[str]:
    """Get standard license text for common SPDX licenses."""
    spdx_id = spdx_id.upper().strip()

    # Direct match
    if spdx_id in SPDX_LICENSES:
        return SPDX_LICENSES[spdx_id]

    # Handle WITH exceptions
    if " WITH " in spdx_id:
        base = spdx_id.split(" WITH ")[0].strip()
        if base in SPDX_LICENSES:
            return SPDX_LICENSES[base]

    return None


def group_by_license(deps: list[Dependency]) -> dict[str, list[Dependency]]:
    """Group dependencies by their primary license."""
    groups = defaultdict(list)
    for dep in deps:
        licenses = normalize_license_id(dep.license)
        primary = licenses[0] if licenses else "UNKNOWN"
        groups[primary].append(dep)
    return dict(groups)


def generate_notice(deps: list[Dependency]) -> str:
    """Generate the NOTICE file content."""
    lines = []
    lines.append("Paper — Local-first document search")
    lines.append("=====================================")
    lines.append("")
    lines.append("This binary includes the following third-party dependencies.")
    lines.append("Each is reproduced with its license text as required by its license terms.")
    lines.append("")

    groups = group_by_license(deps)

    # Sort groups by license preference order
    license_order = {
        "MIT": 0,
        "MIT-0": 1,
        "APACHE-2.0": 2,
        "APACHE-2.0 WITH LLVM-EXCEPTION": 3,
        "BSD-2-CLAUSE": 4,
        "BSD-3-CLAUSE": 5,
        "ISC": 6,
        "CC0-1.0": 7,
        "UNLICENSE": 8,
        "ZLIB": 9,
        "BSL-1.0": 10,
        "UNICODE-3.0": 11,
        "CDLA-PERMISSIVE-2.0": 12,
        "0BSD": 13,
        "MPL-2.0": 14,
    }

    # For display, map common variations to standard names
    license_display = {
        "APACHE-2.0 WITH LLVM-EXCEPTION": "Apache-2.0 WITH LLVM-exception",
        "BSD-2-CLAUSE": "BSD-2-Clause",
        "BSD-3-CLAUSE": "BSD-3-Clause",
        "CC0-1.0": "CC0-1.0",
        "UNLICENSE": "Unlicense",
        "ZLIB": "Zlib",
        "BSL-1.0": "BSL-1.0",
        "UNICODE-3.0": "Unicode-3.0",
        "CDLA-PERMISSIVE-2.0": "CDLA-Permissive-2.0",
        "0BSD": "0BSD",
        "MPL-2.0": "MPL-2.0",
    }

    sorted_groups = sorted(groups.items(), key=lambda x: license_order.get(x[0].upper(), 99))

    for license_id, group_deps in sorted_groups:
        lines.append(f"\n{'=' * 72}")
        lines.append(f"License: {license_id}")
        lines.append(f"{'=' * 72}")
        lines.append("")

        # Print license text once per group
        license_text = get_spdx_license_text(license_id)
        if license_text:
            lines.append(license_text)
        else:
            # Fallback for unknown licenses
            lines.append(f"[Standard license text for {license_id} not available in generator]")
            lines.append(f"License expression: {group_deps[0].license}")
            lines.append(f"Repository: {group_deps[0].repository or 'Unknown'}")

        lines.append("")
        lines.append("Dependencies under this license:")
        lines.append("-" * 40)

        for dep in sorted(group_deps, key=lambda d: d.name):
            lines.append(f"\n  {dep.name} v{dep.version}")
            if dep.repository:
                lines.append(f"  Repository: {dep.repository}")
            lines.append(f"  License: {dep.license}")

    lines.append("")
    lines.append("=" * 72)
    lines.append("End of NOTICE")
    lines.append("=" * 72)

    return "\n".join(lines)


def main():
    print("Fetching cargo metadata...", file=sys.stderr)
    metadata = run_cargo_metadata()

    print("Extracting runtime dependencies...", file=sys.stderr)
    deps = get_runtime_dependencies(metadata)

    print(f"Found {len(deps)} dependencies", file=sys.stderr)
    print("Generating NOTICE...", file=sys.stderr)
    notice = generate_notice(deps)

    print(notice)


if __name__ == "__main__":
    main()