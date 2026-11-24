{-# LANGUAGE GADTs #-}
data T where
  First, Second ::
    { item :: Int
    , item :: Bool
    } -> T

main :: IO ()
main = pure ()
