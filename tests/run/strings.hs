main :: IO ()
main = do
  print "tab\there \"quoted\" back\\slash"
  print "\1234 unicode \955 \DEL \200"
  print "\SOH\&9 and \SO\&H"
  print (words "  the quick  brown\tfox\n")
  print (unwords ["a", "b", "c"])
  print (lines "one\ntwo\n\nthree")
  print (unlines ["x", "y"])
  print ('\'', '"', '\n', '\0')
  print (show 'a' ++ show "b")
  print (read "\"escaped\\n\"" :: String)
