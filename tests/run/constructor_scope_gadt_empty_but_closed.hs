{-# LANGUAGE GADTs #-}
data T where
  T :: forall . (Int -> T)

main :: IO ()
main = case T 3 of T value -> print value
