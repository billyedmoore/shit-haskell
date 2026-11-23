{-# LANGUAGE DuplicateRecordFields #-}
data T = A { item :: Int, item :: Int }
main :: IO ()
main = pure ()
