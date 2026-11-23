{-# LANGUAGE GADTs, PartialTypeSignatures #-}
import Data.Proxy (Proxy)

data T where
  T :: Proxy _ -> T

main :: IO ()
main = pure ()
