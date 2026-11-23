{-# LANGUAGE GADTs #-}
data T where
  T :: forall a. Show a => { field :: a } -> T

main :: IO ()
main = case T True of T value -> print value
