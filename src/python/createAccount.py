try:
    from selenium import webdriver
    from selenium.webdriver.common.by import By
    from selenium.webdriver.chrome.service import Service
    from selenium.webdriver.support.ui import WebDriverWait
    from selenium.webdriver.support import expected_conditions as EC
    import random, time, re, os,sys
except:
    import os
    print("[ERROR] Couldn't load all libraries. Attempting to download them...")
    os.system("pip install selenium")
    from selenium import webdriver
    from selenium.webdriver.common.by import By
    from selenium.webdriver.chrome.service import Service
    from selenium.webdriver.support.ui import WebDriverWait
    from selenium.webdriver.support import expected_conditions as EC
    import random, time, re, os,sys
config=sys.argv
usernamePrefix=config[1]
emailPrefix=config[2]
emailDomain=config[3]
qualify=config[4]
loop=config[5]

print(f"[CONFIG] Loaded Config:\n > Username Prefix: {usernamePrefix}\n > Email: {emailPrefix}00000@{emailDomain}\n > Looping: {loop}\n > Qualifying for the leaderboards: {qualify}\n\n")

loop = True if loop == "true" else False
qualify = True if qualify == "true" else False

script_dir = os.path.dirname(os.path.realpath(__file__))
driver_path = os.path.join(script_dir, "driver", "chromedriver.exe")
js_script_path = os.path.join(script_dir, "js", "pronunciation.js")
accounts_path = os.path.join(script_dir, "logs", "accounts.txt")

def accountDetails(email,password,cookie):
    try:
        with open(accounts_path, 'a', encoding='utf-8') as file:
            file.write(f"{email}:{password}:{cookie}\n\n")
            print(f"[ACCOUNT] Save account details:\n > Email: {email}\n > Passowrd: {password}\n > Cookie: {cookie}")
    except Exception as e:
        print(f"Error saving to file: {e}")


def createTheDamnAccount():
    service = Service(executable_path=driver_path)
    options = webdriver.ChromeOptions()
    options.add_argument("--mute-audio")
    driver = webdriver.Chrome(service=service, options=options)
    driver.get("https://duolingo.com")


    try:
        have_account_button = WebDriverWait(driver, 10).until(
            EC.element_to_be_clickable((By.CSS_SELECTOR, '[data-test="have-account"]'))
        )
        have_account_button.click()
        sign_up_button = WebDriverWait(driver, 10).until(
            EC.element_to_be_clickable((By.CSS_SELECTOR, '[data-test="sign-up-button"]'))
        )
        sign_up_button.click()
    except Exception as e:
        print(f"Error: {e}")


    visible_inputs = [
        input_element for input_element in driver.find_elements(By.TAG_NAME, "input")
    ]

    username=f"{usernamePrefix}{random.randint(100000,999999)}"
    email=f"{emailPrefix}{random.randint(1000000,9999999)}@{emailDomain}"
    password=f"{random.randint(1000,9999)}{username}{random.randint(1000,9999)}"

    print("\n[ACCOUNT] Registering")
    for index, input_element in enumerate(visible_inputs, start=1):
        data_test = input_element.get_attribute('data-test')
        if data_test == "age-input":
            input_element.send_keys(random.randint(18,56))
        elif data_test == "full-name-input":
             input_element.send_keys(username)
        elif data_test == "email-input":
            input_element.send_keys(email)
        elif data_test == "password-input":
            input_element.send_keys(password)
        print(f" > Input {index}: Filled '{data_test.replace("-"," ")}'")
    print(f"\n[ACCOUNT] Signed up with:\n > Email: {email}\n > Password: {password}\n\n")

    register_button = WebDriverWait(driver, 10).until(
           EC.element_to_be_clickable((By.CSS_SELECTOR, '[data-test="register-button"]'))
       )
    register_button.click()

    lang = WebDriverWait(driver, 10).until(
           EC.element_to_be_clickable((By.CSS_SELECTOR, '[role="radio"]'))
       )
    lang.click()
    WebDriverWait(driver, 10).until(
           EC.element_to_be_clickable((By.CSS_SELECTOR, '[data-test="funboarding-continue-button"]'))
       ).click()



    time.sleep(5)
    cookies = driver.execute_script("return document.cookie;")

    jwtToken=re.search(r'jwt_token=([\w-]+\.[\w-]+\.[\w-]+)', cookies).group(1)
    print(f"\n[TOKEN] Found token: {jwtToken}\n")

    time.sleep(0.5)
    driver.execute_script("window.location.href = 'https://www.duolingo.com/alphabets/en/pronunciation';")
    time.sleep(2)
    with open(js_script_path, "r", encoding="utf-8") as file:
        js_script = file.read()

    theRange=1
    if qualify:
        theRange=10
    for i in range(theRange):
        try:
            driver.execute_script(js_script)
        except: 
            ""
        time.sleep(13)
        driver.execute_script("window.location.href = 'https://www.duolingo.com/alphabets/en/pronunciation';")
    accountDetails(email,password,cookies)
    driver.quit()

if loop:
    while True:
        createTheDamnAccount()
else:
    createTheDamnAccount()

print("\nTask finished closing in 10s...")
time.sleep(10)