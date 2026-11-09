data Person = Person
  { personName :: String
  , personAge :: Int
  , personEmail :: Maybe String
  } deriving (Show, Eq)

birthday :: Person -> Person
birthday p = p { personAge = personAge p + 1 }

main :: IO ()
main = do
  let alice = Person { personName = "Alice", personAge = 30, personEmail = Nothing }
      bob = Person "Bob" 25 (Just "bob@example.com")
  print alice
  print (birthday bob)
  putStrLn (personName alice)
  print (alice == alice { personAge = 30 })
  print (map personAge [alice, bob])
