{-# LANGUAGE GADTs, PartialTypeSignatures #-}
data T where
  T :: Int -> T

value :: _
value = 3 :: Int

main :: IO ()
main = case T value of T n -> print n
