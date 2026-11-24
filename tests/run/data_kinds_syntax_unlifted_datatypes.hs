{-# LANGUAGE Haskell2010, UnliftedDatatypes, PolyKinds #-}

import Data.Proxy

value :: Proxy 12
value = Proxy

main :: IO ()
main = pure ()
