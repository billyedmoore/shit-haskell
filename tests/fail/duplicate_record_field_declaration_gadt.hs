{-# LANGUAGE GADTs #-}
data T where
  T :: { item :: Int, item :: Int } -> T
main :: IO ()
main = pure ()
