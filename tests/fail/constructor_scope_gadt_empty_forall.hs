{-# LANGUAGE GADTs #-}
data T where
  T :: forall . missing -> T
main :: IO ()
main = pure ()
