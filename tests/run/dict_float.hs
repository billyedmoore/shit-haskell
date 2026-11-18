{-# LANGUAGE ScopedTypeVariables #-}
-- Dictionaries made inside functions from constant types (`Typeable (Maybe [Int])`,
-- `Show [Int]`) are built once, not at each call: many typeOf comparisons run fast (uniplate's
-- Data traversals in hlint do this). Dictionaries that use the function's own stay put.
import Data.Typeable
import Data.List (foldl')
import qualified Data.Map as M

data T a = T a deriving Show

isMaybeList :: Typeable a => a -> Int
isMaybeList x = if typeOf x == typeRep (Proxy :: Proxy (Maybe [Int])) then 1 else 0

-- `Show [a]` is built from the argument's dictionary: not constant.
showBoth :: forall a. Show a => a -> String
showBoth x = show [x, x] ++ show (Just [x])

-- A constant dictionary under a lambda, used by a local function.
count :: [Int] -> M.Map String Int
count = foldr (\x -> M.insertWith (+) (show [x `mod` 3]) 1) M.empty

main :: IO ()
main = do
  print (foldl' (\acc i -> acc + isMaybeList (Just [i]) + isMaybeList (T i) + isMaybeList "s") 0 [1 .. 200000 :: Int])
  putStrLn (showBoth 'c' ++ showBoth (3 :: Int))
  print (count [1 .. 100])
  print (typeOf (Just [T 'x']), typeRep (Proxy :: Proxy (Either Int)))
