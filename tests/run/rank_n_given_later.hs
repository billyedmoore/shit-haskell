-- A constrained (rank-N) argument whose dictionary's type is fixed only by a later argument
-- (hackage-security's checked exceptions: `catchChecked (Right <$> act) (return . Left)`), or
-- only by later uses of the result (`tryChecked $ tryChecked $ act`, then `case r of ...`).
{-# LANGUAGE RankNTypes, ScopedTypeVariables #-}

class Throws e where

newtype Catch e = Catch e
instance Throws (Catch e) where

withThrows :: forall a e. (Throws e => IO a) -> (e -> IO a) -> IO [a]
withThrows _ handler = return []

tryIt :: (Throws e => IO a) -> IO [Either e a]
tryIt act = withThrows (Right <$> act) (return . Left)

data A = A deriving Show
data B = B deriving Show

throwsA :: Throws A => Int -> IO Int
throwsA n = return (n + 1)

throwsB :: Throws B => Int -> IO Int
throwsB n = return (n * 2)

both :: (Throws A, Throws B) => IO Int
both = throwsA 1 >>= throwsB

nested :: IO String
nested = do
  r <- tryIt $ tryIt $ both
  case r of
    [] -> return "none"
    (Left a : _) -> return (show (a :: A))
    (Right [Left b] : _) -> return (show (b :: B))
    _ -> return "other"

main :: IO ()
main = do
  r <- tryIt (return 'x') :: IO [Either Int Char]
  print r
  nested >>= putStrLn
