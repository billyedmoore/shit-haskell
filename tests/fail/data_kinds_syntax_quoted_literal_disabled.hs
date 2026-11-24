{-# LANGUAGE TemplateHaskell, NoDataKinds #-}

import Language.Haskell.TH (Q, Type)

value :: Q Type
value = [t| 12 |]

main :: IO ()
main = pure ()
