extern crate serde;
extern crate serde_json;
extern crate openssl;
extern crate ring;

#[macro_use]
extern crate serde_derive;

mod shamirsecret;
use shamirsecret::ShamirSecret;

use openssl::rand::rand_bytes;
use openssl::sha::sha256;

use std::fs::File;
use std::io::{Read, Write};
use std::collections::HashMap;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize(){

        let json = File::open("tests/accounts.json").unwrap();
        let accounts = serde_json::from_reader::<File, AccountsWrapper>(json).unwrap();

        println!("{:?}", accounts);
    }

    #[test]
    fn test_serialize(){
        let my_account = Accounts {
            id: 1,
            username: String::from("test_username"),
            salt: String::from("abcdefghijk1234567890"),
            sharenumber: 1,
            passhash: String::from("testpasswordhash")
        };

        let mut account_hash: HashMap<i64, Accounts> = HashMap::new();

        account_hash.insert(my_account.id, my_account);

        let accounts = AccountsWrapper {
            accounts: account_hash
        };

        // TODO: serialize to json
    }

    #[test]
    fn test_create_pph(){
        let mut pph = PolyPasswordHasher::new(10, None, None);

        pph.create_account(String::from("admin"), String::from("correct horse"), 5);
        pph.create_account(String::from("root"), String::from("battery staple"), 5);
        pph.create_account(String::from("superuser"), String::from("purple monkey dishwasher"), 5);

        pph.create_account(String::from("alice"), String::from("kitten"), 1);
        pph.create_account(String::from("bob"), String::from("puppy"), 1);
        pph.create_account(String::from("charlie"), String::from("velociraptor"), 1);
        pph.create_account(String::from("dennis"), String::from("menace"), 0);
        pph.create_account(String::from("eve"), String::from("iamevil"), 0);



    }
}


pub struct PolyPasswordHasher {

    // Define the threshold we would need for secret sharing.
    // This parameter will be passed to the shamirsecret library
    threshold: u8,

    // accountdict hosts a salt, sharenumber, and hash,
    // which is the salted pswd ^ secretshare.
    // An accountdict should contain the following:
    /*
        "accounts" : [
           {
              "id": 1,
              "username": "my_username",
              "salt": "abcdefghijk...",
              "sharenumber": "1",
              "passhash": "abcdefghijk...",
           },
           ... etc.
        ]
    */
    accountdict: Option<AccountsWrapper>,

    // Set this as the ShamirSecret object we will be using
    shamirsecretobj: Option<ShamirSecret>,

    // We want to know if the secret value is known, and if so, should
    // a password file be used?
    knownsecret: bool,

    // Length of salt in bytes
    saltsize: u8,

    // Number of bytes of data used for partial verification
    partialbytes: Option<u8>,

    // support for thresholdless encryption.
    thresholdlesskey: Option<Vec<u8>>,

    // Specifies number of used shares.
    nextavailableshare: u8
}

impl PolyPasswordHasher {

    // Create a new PolyPasswordHasher object with/without the option of providing a file
    pub fn new(threshold: u8, passwordfile: Option<String>, partialbytes: Option<u8>) -> PolyPasswordHasher {

        // Variable to hold ShamirSecret object
        let shamirsecretobj: ShamirSecret;

        // Variable to hold nextavailableshare
        let mut nextavailableshare: u8 = 1;

        // If the user does not specify a password file...
        if let None = passwordfile {

            // Create a new array to fill with 32 random bytes
            let mut buffer = [0u8; 256];
            rand_bytes(&mut buffer).unwrap();


            let thresholdlesskey = unsafe {
                String::from_utf8_unchecked(buffer.to_vec())
            };

            // Create new ShamirSecret object
            shamirsecretobj = ShamirSecret::new(threshold,
                Some(thresholdlesskey.to_owned()));

            // Return the new struct
            return PolyPasswordHasher {
                threshold: threshold,
                accountdict: None,
                shamirsecretobj: Some(shamirsecretobj),
                knownsecret: true,
                saltsize: 16u8,
                partialbytes: partialbytes,
                thresholdlesskey: Some(thresholdlesskey.into_bytes()),
                nextavailableshare: 1
            };

        }

        // If a passwordfile is specified, however...
        shamirsecretobj = ShamirSecret::new(threshold, None);

        // Open file and store content from passwordfile
        let mut file = File::open(passwordfile.unwrap()).unwrap();
        let mut raw_content = String::new();
        let _ = file.read_to_string(&mut raw_content);

        // Use serde to deserialize data from file
        let accountdict = serde_json::from_str::<AccountsWrapper>(&raw_content).unwrap();

        // Grab the id, and the Account struct for each account within the HashMap
        for (_id, account) in accountdict.accounts.iter() {
            nextavailableshare = std::cmp::max(nextavailableshare, account.sharenumber);
        }

        nextavailableshare += 1;

        // Finally, return the new PolyPasswordHasher struct
        PolyPasswordHasher {
            threshold: threshold,
            accountdict: Some(accountdict),
            shamirsecretobj: Some(shamirsecretobj),
            knownsecret: false,
            saltsize: 16u8,
            partialbytes: partialbytes,
            thresholdlesskey: None,
            nextavailableshare: nextavailableshare
        }

    }

    pub fn create_account(&mut self, username: String, password: String, shares: u8) {

        // Borrow accountdict as its own variable binding
        let mut accountdict = self.accountdict.clone().unwrap();

        if self.knownsecret == false {
            panic!("Password file is not unlocked!");
        }

        // Iterate over "dict", check if username exists
        for (_id, account) in accountdict.accounts.iter() {
            if account.username == username {
                panic!("Username already exists!");
            }
        }

        // TODO: implement hasher for thresholdless support

        for sharenumber in self.nextavailableshare..(self.nextavailableshare + shares) {

            let shamirsecretdata = self.shamirsecretobj.clone().unwrap().compute_share(sharenumber);

            // Create a new buffer (as a vector) to fill with random data
            let mut salt_buffer: Vec<u8> = vec![0u8; self.saltsize as usize];
            rand_bytes(&mut salt_buffer).unwrap();

            // Convert to string, pass as salt
            let salt: String = String::from_utf8(salt_buffer).unwrap();

            // Concatenate the salt and the password
            let saltpass: String = format!("{}{}", salt, password);

            // Create a salted password hash
            let saltedpasswordhash: [u8; 32] = sha256(&saltpass.as_bytes());

            let mut passhash: Vec<u8> = do_bytearray_xor(saltedpasswordhash.to_vec(), shamirsecretdata);

            passhash.push(saltedpasswordhash[saltedpasswordhash.len() - self.partialbytes.unwrap() as usize]);

            // Create a new entry for the "dict"
            let new_account = Accounts {
                id: 0, // TODO: change!
                username: username.clone(),
                salt: salt,
                sharenumber: sharenumber,
                passhash: String::from_utf8(passhash).unwrap()
            };

            // Add to accountdict
            accountdict.accounts.insert(new_account.id, new_account);
        }

        // Iterate nextavailableshare
        self.nextavailableshare += shares;
    }

    pub fn is_valid_login(&self, username: String, password: String) -> bool {

        // Borrow accountdict as its own variable binding
        let accountdict = self.accountdict.clone().unwrap();

        if self.knownsecret == false && self.partialbytes.unwrap() == 0 {
            panic!("Password File is not unlocked and partial verification is disabled!");
        }

        // Collect all usernames into a vector
        let mut username_vec: Vec<String> = vec![];
        for (_id, account) in accountdict.accounts.iter() {
            username_vec.push(account.clone().username);
        }

        if !username_vec.contains(&username){
            panic!("Unknown user {}", username);
        }

        for (_id, account) in accountdict.accounts.iter(){

             let saltpass: String = format!("{}{}", account.salt, password);

             let saltedpasswordhash: [u8; 32] = sha256(&saltpass.as_bytes());

             if !self.knownsecret{

                 let saltedcheck = saltedpasswordhash[saltedpasswordhash.len() - (self.partialbytes.unwrap() as usize)];

                 let entrycheck = account.clone().passhash.into_bytes()[account.clone().passhash.len()] - self.partialbytes.unwrap();

                 return saltedcheck == entrycheck;
             }


             let sharedata = do_bytearray_xor(saltedpasswordhash.to_vec(),
                 account.clone().passhash.into_bytes()[0..(account.clone().passhash.len() - self.partialbytes.unwrap() as usize)].to_vec());

              // TODO : implement thresholdless account support

              let mut share: Vec<u8> = vec![account.sharenumber];

              for element in sharedata.iter() {
                  share.push(*element);
              }

              // Create a clone of shamir object
              let shamir = self.shamirsecretobj.clone().unwrap();

              return shamir.is_valid_share(share);
        }

        false
    }

    pub fn write_password_data(&mut self, passwordfile: String) {

        // Borrow accountdict as its own variable binding
        let accountdict = self.accountdict.clone().unwrap();

        if self.threshold >= self.nextavailableshare {
            panic!("Would write undecodable password file. Must have more shares before writing.");
        }

        // Open file and store content from passwordfile
        let mut file = File::open(passwordfile.as_str()).unwrap();

        // Convert accountdict to a string
        let raw_accountdict = serde_json::to_string::<AccountsWrapper>(&accountdict).unwrap();

        // Write data to file
        let _ = file.write_all(raw_accountdict.as_bytes());

    }

    pub fn unlock_password_data(&mut self, logindata: Vec<(String, String)>) {

        if self.knownsecret {
            panic!("Password File is already unlocked!");
        }

        // Borrow accountdict as its own variable binding
        let accountdict = self.accountdict.clone().unwrap();

        let mut sharelist = vec![];

        for (username, password) in logindata {

            // Collect all usernames into a vector for comparison
            let mut username_vec: Vec<String> = vec![];
            for (_id, account) in accountdict.accounts.iter() {
                username_vec.push(account.clone().username);
            }

            if !username_vec.contains(&username){
                panic!("Unknown user {}", username);
            }

            for (_id, account) in accountdict.accounts.iter() {

                if account.username == username {

                    if account.sharenumber == 0 {
                        continue;
                    }

                    // Concatenate the salt and the password
                    let saltpass: String = format!("{}{}", account.salt, password);

                    let thissaltedpasswordhash: [u8; 32] = sha256(&saltpass.as_bytes());

                    let sharedata = do_bytearray_xor(thissaltedpasswordhash.to_vec(),
                        account.clone().passhash.into_bytes()[0..(account.clone().passhash.len() - self.partialbytes.unwrap() as usize)].to_vec());

                    let mut thisshare = vec![account.sharenumber];
                    thisshare.extend(sharedata.iter().cloned());

                    sharelist.push(thisshare);

                }
            }
        }

        self.shamirsecretobj.clone().unwrap().recover_secretdata(sharelist);

        self.thresholdlesskey = Some(self.shamirsecretobj.clone().unwrap().secretdata.unwrap().into_bytes());

        self.knownsecret = true;
    }

}



/* ========================================================
   Structs defined for JSON serialization/deserialization
   ========================================================*/


#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AccountsWrapper {
    #[serde(with = "accounts")]
    pub accounts: HashMap<i64, Accounts>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Accounts {
    // This is the HashMap key.
    id: i64,
    username: String,
    salt: String,
    sharenumber: u8,
    passhash: String
}

/* ==============================================
   Helper Serialization/Deserialization mod for
   JSON / HashMap conversions
   ==============================================*/

mod accounts {
    use super::Accounts;

    use std::collections::HashMap;

    use serde::ser::Serializer;
    use serde::de::{Deserialize, Deserializer};

    pub fn serialize<S>(map: &HashMap<i64, Accounts>, serializer: S) -> Result<S::Ok, S::Error>
        where S: Serializer
    {
        serializer.collect_seq(map.values())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<HashMap<i64, Accounts>, D::Error>
        where D: Deserializer<'de>
    {
        let mut map = HashMap::new();
        for item in Vec::<Accounts>::deserialize(deserializer)? {
            map.insert(item.id, item);
        }
        Ok(map)
    }

}


/* ==============================================
   Private Math Function for XOR hashes
   ==============================================*/

fn do_bytearray_xor(a: Vec<u8>, b: Vec<u8>) -> Vec<u8> {
    if a.len() != b.len() {
        panic!("{:?} {:?}, {:?} {:?}", a.len(), b.len(), a, b);
    }

    let mut result = vec![];

    for position in 0..a.len() {
        result.push(a[position] ^ b[position]);
    }

    result
}
