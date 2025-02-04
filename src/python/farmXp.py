import re,time,requests,json,sys,os
config=sys.argv
lessonsPerAccount=config[1]
legit=config[2]
loop=config[3]

lessonsPerAccount = int(lessonsPerAccount)
loop = True if loop == "true" else False
legit = True if legit == "true" else False

print(f"[CONFIG] Loaded config:\n > Lessons per account: {lessonsPerAccount}\n > Legit mode: {legit}\n > Looping: {loop}\n")

if legit == False: print("[INFO] Rage mode for XP (also known as Legit disabled) is not yet supported. Nothing will change.\n[INFO] You do not need to modify it; for now, the only thing it will do is log this message.\n")

email_cookie_list = []
repeat = 1
if lessonsPerAccount > 0:
    repeat = lessonsPerAccount

script_dir = os.path.dirname(os.path.realpath(__file__))
accounts_path = os.path.join(script_dir, "logs", "accounts.txt")


with open(accounts_path, "r", encoding="utf-8") as file:
    for line in file:
        match = re.match(r"([^:]+):[^:]+:(.*)", line.strip())
        if match:
            email, cookie = match.groups()
            email_cookie_list.append([email, cookie])

for t in range(len(email_cookie_list)):
    i =email_cookie_list[t]
    login = {
        "username":i,
        "token": re.search(r"jwt_token=([^;]+)", i[1]).group(1),
        "fromLanguage": "pl",
        "learningLanguage": "en"
    }   
    string=""
    if email_cookie_list[t] != email_cookie_list[0]:
        string="\n\n"
    print(f"{string}[{t+1}/{len(email_cookie_list)}] Logged in as {login['username']}")
    for a in range(repeat):
        headers = {
            "Content-Type": "application/json",
            "Authorization": f"Bearer {login['token']}",
            "User-Agent": "Mozilla/5.0"  # Modify this if needed
        }

        body = {
            "challengeTypes": [
                "assist", "characterIntro", "characterMatch", "characterPuzzle",
                "characterSelect", "characterTrace", "characterWrite",
                "completeReverseTranslation", "definition", "dialogue",
                "extendedMatch", "extendedListenMatch", "form", "freeResponse",
                "gapFill", "judge", "listen", "listenComplete", "listenMatch",
                "match", "name", "listenComprehension", "listenIsolation",
                "listenSpeak", "listenTap", "orderTapComplete", "partialListen",
                "partialReverseTranslate", "patternTapComplete", "radioBinary",
                "radioImageSelect", "radioListenMatch", "radioListenRecognize",
                "radioSelect", "readComprehension", "reverseAssist",
                "sameDifferent", "select", "selectPronunciation",
                "selectTranscription", "svgPuzzle", "syllableTap",
                "syllableListenTap", "speak", "tapCloze", "tapClozeTable",
                "tapComplete", "tapCompleteTable", "tapDescribe", "translate",
                "transliterate", "transliterationAssist", "typeCloze",
                "typeClozeTable", "typeComplete", "typeCompleteTable",
                "writeComprehension"
            ],
            "fromLanguage": login["fromLanguage"],
            "isFinalLevel": False,
            "isV2": True,
            "juicy": True,
            "learningLanguage": login["learningLanguage"],
            "smartTipsVersion": 2,
            "type": "GLOBAL_PRACTICE"
        }

        # First POST request to start the session
        response = requests.post(
            "https://www.duolingo.com/2017-06-30/sessions",
            headers=headers,
            data=json.dumps(body)
        )

        if response.status_code == 200:
            session = response.json()
            session_id = session.get('id')

            if session_id:
                # Second PUT request to complete the session
                completion_body = {
                    **session,
                    "heartsLeft": 0,
                    "startTime": (int(time.time() * 1000) - 60000) // 1000,
                    "enableBonusPoints": True,
                    "endTime": int(time.time()),
                    "failed": False,
                    "maxInLessonStreak": 9,
                    "shouldLearnThings": True
                }

                requests.put(
                    f"https://www.duolingo.com/2017-06-30/sessions/{session_id}",
                    headers=headers,
                    data=json.dumps(completion_body)
                )

        if a+1 != repeat:
            sys.stdout.write(f"\r> [{a+1}/{repeat}] +{13*(a+1)}xp")
        else: 
            sys.stdout.write(f"\r> [{a+1}/{repeat}] Gained {13*repeat}xp")

        sys.stdout.flush()
print("\nTask finished closing in 10s...")
time.sleep(10)