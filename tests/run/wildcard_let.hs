{-# LANGUAGE RecordWildCards, PatternSynonyms #-}
-- `T{..}` as a let or where pattern binds the fields as variables (hlint's Hint.Restrict), in
-- do blocks, let expressions and where clauses.
pattern Pair :: Int -> Int -> (Int, Int)
pattern Pair {x, y} = (x, y)

data Item = Item { name :: String, size :: Int, note :: Maybe String }

describe :: Item -> Maybe String
describe it = do
  let Item{..} = it
  fmap (\n -> name ++ ": " ++ n ++ " " ++ show size) note

area :: Item -> Int
area it = let Item{..} = it in size * size

twice :: Item -> Int
twice it = size + size
  where Item{..} = it

main :: IO ()
main = do
  let i = Item "box" 3 (Just "heavy")
  print (describe i, describe i { note = Nothing })
  print (area i, twice i)
  let Item{..} = i
  putStrLn name
  -- A record pattern synonym's fields, in a comprehension's let (hlint's Test.Annotations).
  print [x + y | p <- [(1, 2), (3, 4 :: Int)], let Pair{..} = p]
