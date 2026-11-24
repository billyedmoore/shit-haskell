{-# LANGUAGE DataKinds, PatternSynonyms #-}

import Data.Proxy
pattern Yes = True
value :: Proxy Yes
value = Proxy

main :: IO ()
main = pure ()
