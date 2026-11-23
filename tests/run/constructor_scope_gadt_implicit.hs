{-# LANGUAGE GADTs #-}
data T where
  T :: x -> T
main :: IO ()
main = pure ()
