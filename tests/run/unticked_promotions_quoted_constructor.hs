{-# LANGUAGE DataKinds, TemplateHaskell #-}

import Language.Haskell.TH (Q, Type)

data D = MkD

value :: Q Type
value = [t| MkD |]

main :: IO ()
main = pure ()
