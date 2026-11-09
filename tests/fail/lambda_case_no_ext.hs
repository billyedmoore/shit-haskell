f :: Maybe Int -> Int
f = \case
  Just n -> n
  Nothing -> 0

main :: IO ()
main = print (f (Just 2))
